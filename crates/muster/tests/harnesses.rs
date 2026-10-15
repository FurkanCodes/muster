//! End-to-end tests for the coding agents beside Claude Code (M6): Codex,
//! Gemini CLI, OpenCode and Copilot CLI. A stand-in for each records how
//! it was started; what each reports is sent the way the real one sends
//! it: Codex's turn notice through the very program its `-c notify` names,
//! and its state in its window title; Gemini CLI's and Copilot CLI's hooks
//! through the commands in the files Muster pointed them at; OpenCode's
//! plugin's reports posted as the plugin posts them.

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
use muster_core::{SessionId, SessionStatus, WaitReason, WorkspaceId};
use serde_json::json;

use support::agents::{AgentLaunch, read};
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

async fn start(
    connection: &mut Connection,
    workspace: WorkspaceId,
    harness: &str,
    prompt: Option<&str>,
) -> SessionId {
    match connection
        .command(Command::StartSession {
            workspace,
            harness: harness.to_owned(),
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

/// Waits until `session`'s status is `wanted`.
async fn expect_until(connection: &mut Connection, session: SessionId, wanted: SessionStatus) {
    expect_event(connection, |frame| match frame {
        ServerFrame::Delta(Delta::SessionStatus {
            session: changed,
            status,
            ..
        }) if *changed == session && *status == wanted => Some(()),
        _ => None,
    })
    .await;
}

/// Stops the daemon and starts it again: running sessions are resumed.
async fn restarted(setup: Setup, connection: Connection) -> (Sandbox, Daemon) {
    drop(connection);
    let Setup {
        sandbox, daemon, ..
    } = setup;
    assert!(daemon.stop().success());
    let daemon = sandbox.start();
    (sandbox, daemon)
}

/// Codex's `notify` program, from the `-c notify=[...]` it was given.
fn notify_program(launch: &AgentLaunch) -> Vec<String> {
    let at = launch
        .args
        .iter()
        .position(|arg| arg.starts_with("notify="))
        .expect("a notify override");
    let value: toml::Value = toml::from_str(&launch.args[at]).unwrap();
    value["notify"]
        .as_array()
        .unwrap()
        .iter()
        .map(|part| part.as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn codex_is_heard_through_its_title_and_turn_notices_and_resumes_its_thread() {
    let setup = setup().await;
    let mut connection = setup.daemon.connect().await;
    connection.subscribe().await.unwrap();
    let session = start(&mut connection, setup.workspace, "codex", Some("fix it")).await;
    let launch = setup.sandbox.wait_for_agent("codex", session, 1).remove(0);
    assert_eq!(launch.args[..2], ["--no-daemon", "-C"]);
    assert!(
        launch
            .args
            .contains(&r#"tui.terminal_title=["activity","run-state"]"#.to_owned()),
        "{:?}",
        launch.args
    );
    assert_eq!(launch.args.last().map(String::as_str), Some("fix it"));
    assert_eq!(launch.token.len(), 64);

    // Its title: working, then waiting on the person, then ready.
    connection.attach(session, None).await.unwrap();
    let title = |text: &str| format!("printf '\\033]0;{text}\\007'\n");
    // Working and waiting in one write, so they reach the daemon in one
    // piece of output, as they can from Codex: each is still a step. The
    // spinner as octal escapes: the stand-in's shell would mangle it typed.
    connection
        .input(
            session,
            b"printf '\\033]0;\\342\\240\\213 Working\\007\\033]0;[ ! ] Action Required\\007'\n",
        )
        .await
        .unwrap();
    let action = SessionStatus::NeedsYou {
        reason: WaitReason::Action,
    };
    expect_until(&mut connection, session, action).await;
    connection
        .input(session, title("Ready").as_bytes())
        .await
        .unwrap();
    expect_until(&mut connection, session, SessionStatus::Finished).await;

    // Its turn notice, through the program it was told to run: the thread
    // to resume.
    let notify = notify_program(&launch);
    assert_eq!(notify[..2], ["sh", "-c"]);
    let notice = json!({
        "type": "agent-turn-complete",
        "thread-id": "019a7c3e-thread",
        "turn-id": "t1",
        "cwd": "/work",
    })
    .to_string();
    launch.run(&notify[2], &[&notify[3], &notice], "");
    // The notice is heard, and its thread kept, before the daemon stops.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let snapshot = setup.daemon.connect().await.subscribe().await.unwrap();
        let kept = snapshot.sessions.iter().find(|stored| stored.id == session);
        if kept.and_then(|stored| stored.resume_id.as_deref()) == Some("019a7c3e-thread") {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the thread was never kept"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    let (sandbox, daemon) = restarted(setup, connection).await;
    let launches = sandbox.wait_for_agent("codex", session, 2);
    assert_eq!(
        launches[1].args[..4],
        ["resume", "--no-daemon", "-C", launches[0].args[2].as_str()]
    );
    assert_eq!(
        launches[1].args.last().map(String::as_str),
        Some("019a7c3e-thread")
    );
    assert!(!launches[1].args.iter().any(|arg| arg == "fix it"));
    drop(daemon);
}

#[tokio::test]
async fn gemini_s_hooks_come_from_muster_s_own_defaults_file() {
    let setup = setup().await;
    let mut connection = setup.daemon.connect().await;
    connection.subscribe().await.unwrap();
    let session = start(&mut connection, setup.workspace, "gemini", None).await;
    let launch = setup.sandbox.wait_for_agent("gemini", session, 1).remove(0);
    let id = launch
        .value("--session-id")
        .expect("--session-id")
        .to_owned();
    assert_eq!(id.len(), 36);

    // Muster's file, in its own runtime directory, holds only its hooks.
    let defaults = launch.gemini_defaults.clone().expect("a defaults file");
    assert!(
        defaults.starts_with(setup.sandbox.paths().runtime_dir),
        "{}",
        defaults.display()
    );
    let defaults: serde_json::Value = serde_json::from_str(&read(&defaults)).unwrap();
    let hook = |event: &str| {
        defaults["hooks"][event][0]["hooks"][0]["command"]
            .as_str()
            .unwrap_or_else(|| panic!("no {event} hook"))
            .to_owned()
    };
    let payload = |event: &str, extra: serde_json::Value| {
        let mut payload = json!({ "session_id": id, "hook_event_name": event, "cwd": "/work" });
        payload
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        payload.to_string()
    };

    launch.run(
        &hook("SessionStart"),
        &[],
        &payload("SessionStart", json!({"source": "startup"})),
    );
    launch.run(
        &hook("BeforeAgent"),
        &[],
        &payload("BeforeAgent", json!({"prompt": "hi"})),
    );
    assert_eq!(
        expect_status(&mut connection, session).await.0,
        SessionStatus::Running
    );
    launch.run(
        &hook("Notification"),
        &[],
        &payload(
            "Notification",
            json!({"notification_type": "ToolPermission", "details": {"type": "exec"}}),
        ),
    );
    assert_eq!(
        expect_status(&mut connection, session).await,
        (
            SessionStatus::NeedsYou {
                reason: WaitReason::Permission
            },
            true
        )
    );
    launch.run(
        &hook("BeforeTool"),
        &[],
        &payload("BeforeTool", json!({"tool_name": "run_shell_command"})),
    );
    assert_eq!(
        expect_status(&mut connection, session).await.0,
        SessionStatus::Running
    );
    launch.run(&hook("AfterAgent"), &[], &payload("AfterAgent", json!({})));
    assert_eq!(
        expect_status(&mut connection, session).await.0,
        SessionStatus::Finished
    );

    let (sandbox, daemon) = restarted(setup, connection).await;
    let launches = sandbox.wait_for_agent("gemini", session, 2);
    assert_eq!(launches[1].value("--resume"), Some(id.as_str()));
    drop(daemon);
}

#[tokio::test]
async fn opencode_s_plugin_is_added_for_the_session_and_its_reports_drive_it() {
    let setup = setup().await;
    let mut connection = setup.daemon.connect().await;
    connection.subscribe().await.unwrap();
    let session = start(&mut connection, setup.workspace, "opencode", Some("fix it")).await;
    let launch = setup
        .sandbox
        .wait_for_agent("opencode", session, 1)
        .remove(0);
    assert_eq!(launch.args[1..], ["--prompt", "fix it"]);

    let config: serde_json::Value =
        serde_json::from_str(launch.opencode_config.as_deref().expect("a config")).unwrap();
    let plugin = config["plugin"][0].as_str().unwrap();
    let plugin = plugin.strip_prefix("file://").expect("a file plugin");
    assert!(read(std::path::Path::new(plugin)).contains("export const MusterStatus"));

    let report = |body: serde_json::Value| launch.post("/hooks/opencode", &launch.token, &body);
    report(json!({"type": "muster.loaded", "session": null, "child": null}));
    report(json!({"type": "chat.message", "session": "ses_root", "child": null}));
    report(json!({"type": "permission.asked", "session": "ses_root", "child": "ses_kid"}));
    assert_eq!(
        expect_status(&mut connection, session).await.0,
        SessionStatus::NeedsYou {
            reason: WaitReason::Permission
        }
    );
    report(json!({"type": "permission.replied", "session": "ses_root", "child": "ses_kid"}));
    assert_eq!(
        expect_status(&mut connection, session).await.0,
        SessionStatus::Running
    );
    report(json!({"type": "session.idle", "session": "ses_root", "child": null}));
    assert_eq!(
        expect_status(&mut connection, session).await.0,
        SessionStatus::Finished
    );

    let (sandbox, daemon) = restarted(setup, connection).await;
    let launches = sandbox.wait_for_agent("opencode", session, 2);
    assert_eq!(launches[1].value("--session"), Some("ses_root"));
    assert_eq!(launches[1].value("--prompt"), None);
    drop(daemon);
}

#[tokio::test]
async fn copilot_s_hooks_come_from_a_plugin_folder_of_muster_s_own() {
    let setup = setup().await;
    let mut connection = setup.daemon.connect().await;
    connection.subscribe().await.unwrap();
    let session = start(&mut connection, setup.workspace, "copilot", Some("fix it")).await;
    let launch = setup
        .sandbox
        .wait_for_agent("copilot", session, 1)
        .remove(0);
    let id = launch.args[0]
        .strip_prefix("--session-id=")
        .expect("--session-id")
        .to_owned();
    assert_eq!(launch.value("--interactive"), Some("fix it"));
    let plugin = std::path::PathBuf::from(launch.value("--plugin-dir").expect("--plugin-dir"));
    assert!(plugin.starts_with(setup.sandbox.paths().runtime_dir));
    assert!(read(&plugin.join("plugin.json")).contains("muster-status"));
    let hooks: serde_json::Value =
        serde_json::from_str(&read(&plugin.join("hooks/hooks.json"))).unwrap();
    let hook = |event: &str| {
        hooks["hooks"][event][0]["bash"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let payload = |extra: serde_json::Value| {
        let mut payload = json!({ "sessionId": id, "timestamp": 1, "cwd": "/work" });
        payload
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        payload.to_string()
    };

    launch.run(
        &hook("permissionRequest"),
        &[],
        &payload(json!({"toolName": "shell"})),
    );
    assert_eq!(
        expect_status(&mut connection, session).await.0,
        SessionStatus::NeedsYou {
            reason: WaitReason::Permission
        }
    );
    launch.run(
        &hook("preToolUse"),
        &[],
        &payload(json!({"toolName": "shell"})),
    );
    assert_eq!(
        expect_status(&mut connection, session).await.0,
        SessionStatus::Running
    );
    launch.run(
        &hook("agentStop"),
        &[],
        &payload(json!({"stopReason": "end_turn"})),
    );
    assert_eq!(
        expect_status(&mut connection, session).await.0,
        SessionStatus::Finished
    );

    let (sandbox, daemon) = restarted(setup, connection).await;
    let launches = sandbox.wait_for_agent("copilot", session, 2);
    assert_eq!(launches[1].args[0], format!("--resume={id}"));
    drop(daemon);
}

#[tokio::test]
async fn a_token_is_heard_only_at_its_own_harness_s_address() {
    let setup = setup().await;
    let mut connection = setup.daemon.connect().await;
    connection.subscribe().await.unwrap();
    let session = start(&mut connection, setup.workspace, "opencode", None).await;
    let launch = setup
        .sandbox
        .wait_for_agent("opencode", session, 1)
        .remove(0);
    // OpenCode's token at Claude Code's address: shaped right, but ignored.
    let claude_stop = json!({"session_id": "s", "hook_event_name": "UserPromptSubmit"});
    launch.post("/hooks/claude", &launch.token, &claude_stop);
    // At its own, it is heard.
    launch.post(
        "/hooks/opencode",
        &launch.token,
        &json!({"type": "chat.message", "session": "ses_1", "child": null}),
    );
    assert_eq!(
        expect_status(&mut connection, session).await.0,
        SessionStatus::Running
    );
}

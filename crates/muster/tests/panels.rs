//! End-to-end tests of plugins' panels (`PRD-15`, `ARC-23`,
//! `ARCHITECTURE.md` §7a) against the real daemon: a panel plugin
//! installed from a `file://` git repository the test makes, whose
//! program prints what Muster told it and its arguments, then waits. It
//! opens beside a session in its worktree, told its branch and session,
//! with only the basics of the environment and what its manifest names;
//! it is never news, runs on through `muster reload`, and ends with its
//! worktree. One whose program is missing is refused, saying how to get
//! it, and so is one turned off.

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

use std::os::unix::fs::PermissionsExt as _;
use std::time::{Duration, Instant};

use muster_client::Connection;
use muster_core::protocol::plugins::{Beside, PluginCommand};
use muster_core::protocol::{Command, Reply, TermSize};
use muster_core::{SessionId, WorkspaceId};

use support::{Sandbox, daemon_pid, git, read_terminal};

const MANIFEST: &str = r#"id = "zoe"
name = "Zoe"
version = "0.1.0"
muster = "0.3.0"
requires = ["sh"]
how_to_get = "cargo install zoe"
env = ["ZOE_THEME"]

[[panels]]
id = "context"
title = "context"
on = "session"
command = ["./context", "{branch}", "{session}"]
key = "Z"
"#;

/// What the panel prints: its arguments, what Muster told it, whether
/// variables it was not to see reached it, and its process.
const CONTEXT: &str = r#"#!/bin/sh
echo "args: $*"
env | grep '^MUSTER_PLUGIN_' | sort
echo "theme=${ZOE_THEME:-unset}"
echo "claude=${CLAUDECODE:-unset}"
echo "hook=${MUSTER_HOOK_TOKEN:-unset}"
echo "note=${PERSONAL_NOTE:-unset}"
echo "pid=$$"
echo "ready-$((40+2))"
exec sleep 60
"#;

/// A git repository holding the panel plugin with `manifest`.
fn plugin_repository(sandbox: &Sandbox, manifest: &str) -> String {
    let root = sandbox.repository("zoe-plugin");
    std::fs::write(root.join("muster-plugin.toml"), manifest).unwrap();
    let program = root.join("context");
    std::fs::write(&program, CONTEXT).unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    git(&root, &["add", "muster-plugin.toml", "context"]);
    git(&root, &["commit", "--quiet", "--message", "the plugin"]);
    format!("file://{}", root.display())
}

fn plugin(sandbox: &Sandbox, args: &[&str]) {
    let output = sandbox
        .command()
        .arg("plugin")
        .args(args)
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "muster plugin {args:?}: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A worktree `work` of a project, and a Claude Code session in it.
async fn chat(connection: &mut Connection, sandbox: &Sandbox) -> (WorkspaceId, SessionId) {
    let root = sandbox.repository("app");
    let Reply::ProjectAdded { id: project } = connection
        .command(Command::AddProject { root, name: None })
        .await
        .unwrap()
    else {
        panic!("expected ProjectAdded");
    };
    let Reply::WorktreeCreated { id: worktree } = connection
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
    let Reply::SessionStarted { id } = connection
        .command(Command::StartSession {
            workspace: worktree,
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
    (worktree, id)
}

fn open(beside: Beside) -> Command {
    Command::Plugin(PluginCommand::OpenPanel {
        plugin: "zoe".to_owned(),
        panel: "context".to_owned(),
        beside,
        size: TermSize::default(),
    })
}

fn alive(pid: &str) -> bool {
    std::process::Command::new("kill")
        .args(["-0", pid])
        .status()
        .unwrap()
        .success()
}

/// Waits up to ten seconds for `done`.
async fn until(what: &str, mut done: impl AsyncFnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done().await {
        assert!(Instant::now() < deadline, "{what}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn a_panel_opens_beside_its_session_told_its_context_and_ends_with_its_worktree_prd_15() {
    let sandbox = Sandbox::new()
        .with_env("ZOE_THEME", "dark")
        .with_env("PERSONAL_NOTE", "not the plugin's");
    let daemon = sandbox.start();
    let pid = daemon.pid();
    plugin(
        &sandbox,
        &["install", &plugin_repository(&sandbox, MANIFEST), "--yes"],
    );
    let mut connection = daemon.connect().await;
    let (worktree, chat) = chat(&mut connection, &sandbox).await;

    let Reply::SessionStarted { id: panel } = connection
        .command(open(Beside::Session(chat)))
        .await
        .unwrap()
    else {
        panic!("expected SessionStarted");
    };
    connection.attach(panel, None).await.unwrap();
    let (text, _) = read_terminal(&mut connection, panel, "ready-42").await;
    // Its placeholders filled in, and what Muster knows in its variables.
    assert!(text.contains(&format!("args: work {chat}")), "{text}");
    assert!(text.contains("MUSTER_PLUGIN_BRANCH=work"), "{text}");
    assert!(
        text.contains(&format!("MUSTER_PLUGIN_SESSION={chat}")),
        "{text}"
    );
    assert!(text.contains("MUSTER_PLUGIN_HARNESS=claude"), "{text}");
    assert!(text.contains("MUSTER_PLUGIN_PROJECT=app"), "{text}");
    // What its manifest names, and nothing of an agent's or Muster's own,
    // nor the person's other variables.
    for told in ["theme=dark", "claude=unset", "hook=unset", "note=unset"] {
        assert!(text.contains(told), "{told}: {text}");
    }
    let program = text
        .lines()
        .find_map(|line| line.trim().strip_prefix("pid="))
        .unwrap()
        .to_owned();

    // Beside its chat, never news.
    let snapshot = connection.subscribe().await.unwrap();
    let stored = snapshot.sessions.iter().find(|s| s.id == panel).unwrap();
    assert_eq!(stored.harness, "plugin:zoe/context");
    assert_eq!(stored.beside, Some(chat));
    assert!(!muster_core::news::needs_attention(stored));
    // Opened again beside it: the one open.
    let again = connection
        .command(open(Beside::Session(chat)))
        .await
        .unwrap();
    assert_eq!(again, Reply::SessionStarted { id: panel });

    // It runs on through a restart in place, the same program.
    let reloaded = sandbox.run(&["reload"]);
    assert!(reloaded.status.success());
    assert_eq!(daemon_pid(daemon.socket()), Some(pid));
    let mut connection = daemon.connect().await;
    let snapshot = connection.subscribe().await.unwrap();
    let stored = snapshot.sessions.iter().find(|s| s.id == panel).unwrap();
    assert_eq!(stored.ended_at, None);
    assert_eq!(stored.beside, Some(chat), "still beside its chat");
    assert!(alive(&program));

    // The worktree goes once its agent is stopped; the panel with it.
    connection
        .command(Command::StopSession { session: chat })
        .await
        .unwrap();
    let remove = Command::RemoveWorktree {
        workspace: worktree,
        force: false,
    };
    until("the worktree is removed", async || {
        connection.command(remove.clone()).await.is_ok()
    })
    .await;
    until("its program ended", async || !alive(&program)).await;
}

#[tokio::test]
async fn a_panel_whose_program_is_missing_or_whose_plugin_is_off_is_refused_saying_why() {
    let sandbox = Sandbox::new();
    let daemon = sandbox.start();
    let manifest = MANIFEST.replace(
        "requires = [\"sh\"]",
        "requires = [\"sh\", \"zoe-not-on-path\"]",
    );
    plugin(
        &sandbox,
        &["install", &plugin_repository(&sandbox, &manifest), "--yes"],
    );
    let mut connection = daemon.connect().await;
    let (_, chat) = chat(&mut connection, &sandbox).await;
    let refused = connection
        .command(open(Beside::Session(chat)))
        .await
        .unwrap_err()
        .to_string();
    assert!(
        refused.contains("Zoe needs zoe-not-on-path, which is not on PATH: cargo install zoe"),
        "{refused}"
    );

    plugin(&sandbox, &["disable", "zoe"]);
    let refused = connection
        .command(open(Beside::Session(chat)))
        .await
        .unwrap_err();
    assert!(
        refused.to_string().contains("zoe is turned off"),
        "{refused}"
    );
    let snapshot = connection.subscribe().await.unwrap();
    assert!(
        snapshot
            .sessions
            .iter()
            .all(|session| !session.harness.starts_with("plugin:")),
        "nothing opened"
    );
}

/// One whole frame from `stream`, header and all, as the daemon wrote it.
async fn raw_frame(stream: &mut tokio::net::UnixStream) -> Vec<u8> {
    use tokio::io::AsyncReadExt as _;
    let read = async {
        let mut header = [0; 4];
        stream.read_exact(&mut header).await.unwrap();
        let mut frame = vec![0; u32::from_le_bytes(header) as usize];
        stream.read_exact(&mut frame).await.unwrap();
        [header.to_vec(), frame].concat()
    };
    tokio::time::timeout(Duration::from_secs(5), read)
        .await
        .expect("the daemon answered in time")
}

/// Whether `bytes` say anything of a plugin's panel: its harness, its
/// key's name, or its place in a layout.
fn names_a_panel(bytes: &[u8]) -> bool {
    ["plugin:", "plugin.", "/ plugin", "plugin)", "| plugin"]
        .iter()
        .any(|word| bytes.windows(word.len()).any(|at| at == word.as_bytes()))
}

#[tokio::test]
async fn a_client_older_than_panels_is_sent_no_panel_and_a_layout_and_keys_it_reads_arc_09() {
    use muster_core::frame::{FrameDecoder, encode};
    use muster_core::protocol::{ClientFrame, Delta, Hello, MIN_PROTOCOL_VERSION, ServerFrame};
    use muster_core::settings::PreferenceChange;
    use tokio::io::AsyncWriteExt as _;

    let sandbox = Sandbox::new();
    let daemon = sandbox.start();
    plugin(
        &sandbox,
        &["install", &plugin_repository(&sandbox, MANIFEST), "--yes"],
    );
    let mut connection = daemon.connect().await;
    let (_, chat) = chat(&mut connection, &sandbox).await;
    let Reply::SessionStarted { id: panel } = connection
        .command(open(Beside::Session(chat)))
        .await
        .unwrap()
    else {
        panic!("expected SessionStarted");
    };
    connection
        .command(Command::ChangePreference(PreferenceChange::Keys {
            action: "plugin.zoe.context".to_owned(),
            keys: Some(vec!["G".to_owned()]),
        }))
        .await
        .unwrap();
    // A client of panels is told of it all.
    let snapshot = connection.subscribe().await.unwrap();
    assert!(snapshot.sessions.iter().any(|session| session.id == panel));
    assert!(snapshot.preferences.keys.contains_key("plugin.zoe.context"));

    // A client of 42, as the UI before panels, reading the frames as sent:
    // this build would complete a layout without the panel as it read it.
    let mut stream = tokio::net::UnixStream::connect(daemon.socket())
        .await
        .unwrap();
    let older = muster_core::plugins::PANELS_PROTOCOL - 1;
    let hello = Hello {
        min_version: MIN_PROTOCOL_VERSION,
        max_version: older,
        client: "before panels".to_owned(),
        capabilities: Vec::new(),
    };
    for frame in [ClientFrame::Hello(hello), ClientFrame::Subscribe] {
        stream.write_all(&encode(&frame).unwrap()).await.unwrap();
    }
    let read = |bytes: &[u8]| -> ServerFrame {
        let mut decoder = FrameDecoder::new();
        decoder.push(bytes);
        decoder.next_frame().unwrap().unwrap()
    };
    let ServerFrame::Welcome(welcome) = read(&raw_frame(&mut stream).await) else {
        panic!("expected Welcome");
    };
    assert_eq!(welcome.version, older);
    let bytes = raw_frame(&mut stream).await;
    assert!(
        !names_a_panel(&bytes),
        "{}",
        String::from_utf8_lossy(&bytes)
    );
    let ServerFrame::Snapshot(snapshot) = read(&bytes) else {
        panic!("expected Snapshot");
    };
    assert!(snapshot.sessions.iter().any(|session| session.id == chat));
    assert!(snapshot.sessions.iter().all(|session| session.id != panel));
    // Nor of a change to the panel; the chat's, as ever.
    connection
        .command(Command::StopSession { session: panel })
        .await
        .unwrap();
    connection
        .command(Command::RenameSession {
            session: chat,
            title: "Fix the login".to_owned(),
        })
        .await
        .unwrap();
    loop {
        let bytes = raw_frame(&mut stream).await;
        assert!(
            !names_a_panel(&bytes),
            "{}",
            String::from_utf8_lossy(&bytes)
        );
        if let ServerFrame::Delta(Delta::SessionUpserted(session)) = read(&bytes)
            && session.id == chat
        {
            break;
        }
    }
}

//! Opening a plugin's panel on the person's word (`PRD-15`, `ARC-23`):
//! the plugin as the registry reads it now, its command filled in from
//! what Muster knows of the session it opens beside, its program found in
//! the plugin's folder, and only the basics of the environment and what
//! its manifest names given; refused, saying why, where it cannot open;
//! opened once beside a session, opened afresh when started again, and
//! ended with its plugin turned off or removed.

use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

use muster_core::protocol::plugins::{Beside, PluginCommand};
use muster_core::protocol::{Command, ErrorBody, ErrorCode, Reply, TermSize};
use muster_core::{Session, SessionId, SessionStatus, Timestamp, WorkspaceId};
use tokio::time::timeout;

use crate::clock::Clock;
use crate::panel_tests::{FixedClock, project, session, spawn, spawn_with, started, state, until};
use crate::store::Store;
use crate::supervisor::SupervisorHandle;

const MANIFEST: &str = r#"id = "demo"
name = "Demo"
version = "0.1.0"
muster = "0.3.0"
requires = ["sh"]
env = ["CARGO_PKG_NAME"]

[[panels]]
id = "context"
title = "context"
on = "session"
command = ["./context", "{branch}", "{session}"]
key = "Z"

[[panels]]
id = "transcript"
title = "transcript"
on = "session"
command = ["./context", "{transcript}"]

[[panels]]
id = "files"
title = "files"
on = "workspace"
command = ["./context", "{workspace}"]
"#;

/// A plugin `demo` in a folder of its own under `dir`, linked and approved
/// in Muster's settings there: its program writes what it was given into
/// the folder, then waits, or ends at once where `QUICK` is in its folder.
fn link(dir: &Path, manifest: &str, enabled: bool) -> PathBuf {
    let folder = dir.join("demo-plugin");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("muster-plugin.toml"), manifest).unwrap();
    let program = folder.join("context");
    std::fs::write(
        &program,
        "#!/bin/sh\nhere=$(dirname \"$0\")\nprintf '%s\\n' \"$@\" > \"$here/args\"\n\
         env > \"$here/env.part\"; mv \"$here/env.part\" \"$here/env\"\n\
         [ -e \"$here/QUICK\" ] && exit 0\nexec sleep 60\n",
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let approved = crate::plugins::sha256(manifest.as_bytes());
    std::fs::write(
        dir.join("settings.toml"),
        format!(
            "[plugins.demo]\nfolder = \"{}\"\nenabled = {enabled}\napproved = \"{approved}\"\n",
            folder.display()
        ),
    )
    .unwrap();
    folder
}

/// A Claude Code chat in `workspace` that ran a turn as `c1`, stored as a
/// restart finds it.
fn chat(dir: &Path, workspace: WorkspaceId) -> Session {
    let chat = Session {
        id: SessionId::new(),
        workspace,
        harness: "claude".to_owned(),
        title: "Fix the login".to_owned(),
        status: SessionStatus::Finished,
        unseen: false,
        resume_id: Some("c1".to_owned()),
        created_at: Timestamp::from_millis(1),
        ended_at: Some(Timestamp::from_millis(2)),
        status_since: Timestamp::from_millis(1),
        activity: None,
        spend: None,
        tidy: None,
        beside: None,
    };
    let mut store = Store::open(&dir.join("muster.db"), FixedClock.now()).unwrap();
    store.insert_session(&chat).unwrap();
    chat
}

async fn open(
    supervisor: &SupervisorHandle,
    panel: &str,
    beside: Beside,
) -> Result<Reply, ErrorBody> {
    supervisor
        .command(Command::Plugin(PluginCommand::OpenPanel {
            plugin: "demo".to_owned(),
            panel: panel.to_owned(),
            beside,
            size: TermSize::default(),
        }))
        .await
}

/// What the plugin's program in `folder` was given, once it has run.
async fn given(folder: &Path) -> (Vec<String>, HashMap<String, String>) {
    let env = timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(env) = std::fs::read_to_string(folder.join("env")) {
                return env;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the panel's program ran");
    let args = std::fs::read_to_string(folder.join("args")).unwrap();
    let env = env
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.to_owned(), value.to_owned()))
        .collect();
    (args.lines().map(str::to_owned).collect(), env)
}

/// A Supervisor on `dir` with the plugin linked and a project whose
/// checkout has a chat in it.
async fn with_a_chat(dir: &Path, manifest: &str) -> (SupervisorHandle, PathBuf, Session) {
    let first = spawn(dir, Vec::new()).await;
    let (_, checkout) = project(dir, &first).await;
    let chat = chat(dir, checkout);
    let folder = link(dir, manifest, true);
    (spawn(dir, Vec::new()).await, folder, chat)
}

#[tokio::test]
async fn a_panel_opens_beside_its_chat_told_what_muster_knows_and_given_only_what_it_names() {
    let dir = tempfile::tempdir().unwrap();
    let (supervisor, folder, chat) = with_a_chat(dir.path(), MANIFEST).await;
    let panel = started(
        open(&supervisor, "context", Beside::Session(chat.id))
            .await
            .unwrap(),
    );
    let (args, env) = given(&folder).await;
    let chat_id = chat.id.to_string();
    // Its placeholders filled in, its program found in its folder.
    assert_eq!(args, ["muster/issue-42-fix", chat_id.as_str()]);
    let value = |key: &str| env.get(key).map(String::as_str);
    assert_eq!(value("MUSTER_PLUGIN_SESSION"), Some(chat_id.as_str()));
    assert_eq!(value("MUSTER_PLUGIN_BRANCH"), Some("muster/issue-42-fix"));
    assert!(env.contains_key("PATH") && env.contains_key("HOME"));
    assert_eq!(value("TERM_PROGRAM"), Some("muster"));
    // Of the person's other variables, only what its manifest names.
    assert_eq!(
        value("CARGO_PKG_NAME").map(str::to_owned),
        std::env::var("CARGO_PKG_NAME").ok()
    );
    assert_eq!(value("CARGO_MANIFEST_DIR"), None);
    assert_eq!(value("MUSTER_HOOK_TOKEN"), None);

    let snapshot = state(&supervisor).await;
    let stored = session(&snapshot, panel);
    assert_eq!(stored.harness, "plugin:demo/context");
    assert_eq!(stored.title, "context");
    assert_eq!(stored.beside, Some(chat.id));
    // Opened again beside the same chat: the one open.
    let again = open(&supervisor, "context", Beside::Session(chat.id))
        .await
        .unwrap();
    assert_eq!(started(again), panel);
    // A workspace's panel, opened from the chat, is on its workspace.
    let files = started(
        open(&supervisor, "files", Beside::Session(chat.id))
            .await
            .unwrap(),
    );
    let snapshot = state(&supervisor).await;
    assert_eq!(session(&snapshot, files).beside, None);
    // What it is beside is kept with it.
    let next = spawn(dir.path(), Vec::new()).await;
    assert_eq!(session(&state(&next).await, panel).beside, Some(chat.id));
    for id in [panel, files] {
        supervisor
            .command(Command::StopSession { session: id })
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn a_panel_that_cannot_open_is_refused_saying_why() {
    let dir = tempfile::tempdir().unwrap();
    let (supervisor, _, chat) = with_a_chat(dir.path(), MANIFEST).await;
    let checkout = chat.workspace;
    let refused = |outcome: Result<Reply, ErrorBody>| outcome.unwrap_err();
    let error = refused(open(&supervisor, "nope", Beside::Session(chat.id)).await);
    assert_eq!(error.code, ErrorCode::NotFound);
    assert!(
        error.message.contains("has no panel called nope"),
        "{error}"
    );
    let error = refused(open(&supervisor, "context", Beside::Workspace(checkout)).await);
    assert_eq!(error.code, ErrorCode::BadRequest);
    assert!(
        error.message.contains("open it beside a session"),
        "{error}"
    );
    // The chat has no transcript yet.
    let error = refused(open(&supervisor, "transcript", Beside::Session(chat.id)).await);
    assert_eq!(error.code, ErrorCode::Conflict);
    assert!(
        error
            .message
            .ends_with("{transcript} has no value: this session has no transcript yet"),
        "{error}"
    );
    let no_such = Command::Plugin(PluginCommand::OpenPanel {
        plugin: "nothing".to_owned(),
        panel: "graph".to_owned(),
        beside: Beside::Workspace(checkout),
        size: TermSize::default(),
    });
    let error = supervisor.command(no_such).await.unwrap_err();
    assert_eq!(error.code, ErrorCode::NotFound);

    // A program it needs is not found: named, with how to get it.
    let needy = MANIFEST.replace(
        "requires = [\"sh\"]",
        "requires = [\"sh\", \"zoe-not-on-path\"]\nhow_to_get = \"cargo install zoe\"",
    );
    link(dir.path(), &needy, true);
    let error = refused(open(&supervisor, "files", Beside::Workspace(checkout)).await);
    assert_eq!(error.code, ErrorCode::Conflict);
    assert_eq!(
        error.message,
        "Demo needs zoe-not-on-path, which is not on PATH: cargo install zoe"
    );
    // Turned off.
    link(dir.path(), MANIFEST, false);
    let error = refused(open(&supervisor, "files", Beside::Workspace(checkout)).await);
    assert!(error.message.starts_with("demo is turned off"), "{error}");
    // Its manifest changed since the person approved it.
    let folder = link(dir.path(), MANIFEST, true);
    std::fs::write(
        folder.join("muster-plugin.toml"),
        MANIFEST.replace("Demo", "Demo, changed"),
    )
    .unwrap();
    let error = refused(open(&supervisor, "files", Beside::Workspace(checkout)).await);
    assert_eq!(error.code, ErrorCode::Conflict);
    assert!(
        error
            .message
            .starts_with("demo cannot run: its manifest changed since you approved it"),
        "{error}"
    );
    assert!(
        state(&supervisor).await.sessions.len() == 1,
        "only the chat"
    );
}

#[tokio::test]
async fn an_ended_panel_starts_again_opened_afresh_beside_its_chat() {
    let dir = tempfile::tempdir().unwrap();
    let (supervisor, folder, chat) = with_a_chat(dir.path(), MANIFEST).await;
    std::fs::write(folder.join("QUICK"), "").unwrap();
    let panel = started(
        open(&supervisor, "context", Beside::Session(chat.id))
            .await
            .unwrap(),
    );
    until(&supervisor, |snapshot| {
        session(snapshot, panel).ended_at.is_some()
    })
    .await;
    std::fs::remove_file(folder.join("env")).unwrap();
    std::fs::remove_file(folder.join("QUICK")).unwrap();
    let again = supervisor
        .command(Command::RestartSession {
            session: panel,
            size: TermSize::default(),
        })
        .await
        .unwrap();
    assert_eq!(started(again), panel);
    let (args, _) = given(&folder).await;
    assert_eq!(args[1], chat.id.to_string(), "beside its chat again");
    let snapshot = state(&supervisor).await;
    assert_eq!(session(&snapshot, panel).ended_at, None);
    supervisor
        .command(Command::StopSession { session: panel })
        .await
        .unwrap();
}

#[tokio::test]
async fn a_plugin_turned_off_or_removed_takes_its_panels_with_it_arc_23() {
    use muster_core::protocol::plugins::ChangedBy;
    let dir = tempfile::tempdir().unwrap();
    let (supervisor, _, chat) = with_a_chat(dir.path(), MANIFEST).await;
    let ended = |panel: SessionId| {
        move |snapshot: &muster_core::protocol::Snapshot| {
            session(snapshot, panel).ended_at.is_some()
        }
    };
    let panel = started(
        open(&supervisor, "context", Beside::Session(chat.id))
            .await
            .unwrap(),
    );
    supervisor
        .command(Command::Plugin(PluginCommand::SetEnabled {
            id: "demo".to_owned(),
            on: false,
            by: ChangedBy::Ui,
        }))
        .await
        .unwrap();
    until(&supervisor, ended(panel)).await;

    supervisor
        .command(Command::Plugin(PluginCommand::SetEnabled {
            id: "demo".to_owned(),
            on: true,
            by: ChangedBy::Ui,
        }))
        .await
        .unwrap();
    let files = started(
        open(&supervisor, "files", Beside::Session(chat.id))
            .await
            .unwrap(),
    );
    supervisor
        .command(Command::Plugin(PluginCommand::Remove {
            id: "demo".to_owned(),
        }))
        .await
        .unwrap();
    until(&supervisor, ended(files)).await;
}

#[tokio::test]
async fn a_panel_is_given_the_secrets_its_plugin_names_as_kept() {
    use crate::secrets::Secrets as _;
    let dir = tempfile::tempdir().unwrap();
    let first = spawn(dir.path(), Vec::new()).await;
    let (_, checkout) = project(dir.path(), &first).await;
    let manifest = MANIFEST.replace("env = [", "secrets = [\"DEMO_KEY\"]\nenv = [");
    let folder = link(dir.path(), &manifest, true);
    let kept = std::sync::Arc::new(crate::secrets::Kept::default());
    kept.set(
        &crate::secrets::plugin_account("demo", "DEMO_KEY"),
        &crate::secrets::Token::new("kept-for-demo"),
    )
    .await
    .unwrap();
    let supervisor = spawn_with(dir.path(), Vec::new(), kept).await;
    let panel = started(
        open(&supervisor, "files", Beside::Workspace(checkout))
            .await
            .unwrap(),
    );
    let (_, env) = given(&folder).await;
    assert_eq!(
        env.get("DEMO_KEY").map(String::as_str),
        Some("kept-for-demo")
    );
    supervisor
        .command(Command::StopSession { session: panel })
        .await
        .unwrap();
}

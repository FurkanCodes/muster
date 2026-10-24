//! A plugin's panel in a workspace (`PRD-15`, `ARC-23`): started where
//! it is told, with what Muster knows of where it is; no agent, never
//! news; ended with its worktree, carried through a restart in place as a
//! panel, and ended by a daemon that crashed, as a shell is.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use muster_core::protocol::{Command, ErrorCode, Reply, Snapshot};
use muster_core::{Session, SessionId, SessionStatus, Timestamp, WorkspaceId};

use crate::clock::Clock;
use crate::git::tests::{git, repository};
use crate::handoff::HandedSession;
use crate::harness::PanelLaunch;
use crate::spend::Places;
use crate::store::{Store, StoreHandle};
use crate::supervisor::{self, SupervisorHandle};
use crate::waiting;

#[derive(Debug)]
pub(crate) struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> Timestamp {
        Timestamp::from_millis(42)
    }
}

/// A Supervisor on `dir`'s store, taking `adopted` over as it starts.
pub(crate) async fn spawn(dir: &Path, adopted: Vec<HandedSession>) -> SupervisorHandle {
    spawn_with(dir, adopted, Arc::new(crate::secrets::Kept::default())).await
}

/// [`spawn`], with plugins' secrets kept in `secrets`.
pub(crate) async fn spawn_with(
    dir: &Path,
    adopted: Vec<HandedSession>,
    secrets: Arc<dyn crate::secrets::Secrets>,
) -> SupervisorHandle {
    let store =
        StoreHandle::spawn(Store::open(&dir.join("muster.db"), FixedClock.now()).unwrap()).unwrap();
    supervisor::spawn(supervisor::Settings {
        store,
        clock: Arc::new(FixedClock),
        worktrees_dir: dir.join("worktrees"),
        data_dir: dir.join("data"),
        hook_socket: dir.join("hooks.sock"),
        hook_queue: tokio::sync::mpsc::channel(1).1,
        delta_capacity: 256,
        timing: muster_state::session::Timing::default(),
        work: Arc::new(crate::runs::NotYetBuilt),
        trackers: Arc::new(crate::trackers::choose::ServiceTrackers {
            secrets: Arc::new(crate::secrets::Kept::default()),
            plugins: crate::trackers::plugin::Plugins::new("muster".into()),
        }),
        config_dir: dir.to_owned(),
        credentials: Arc::new(crate::runs::credentials::FixedCredentials::default()),
        docker: "docker".to_owned(),
        hosts: Arc::new(crate::githost::choose::ServiceHosts {
            secrets: Arc::new(crate::secrets::Kept::default()),
            http: crate::service::http_client(),
        }),
        adopted,
        tasks: crate::tasks::Tasks::default(),
        ticket_plugins: crate::trackers::plugin::Plugins::new("muster".into()),
        places: Places {
            claude_projects: Some(dir.join("claude/projects")),
            codex_sessions: Some(dir.join("codex/sessions")),
            gemini_tmp: None,
        },
        secrets,
        hooks: crate::plugins::hooks::Hooks::idle(),
        repositories: Arc::new(crate::repositories::Gh),
    })
    .await
    .unwrap()
}

/// A repository `app` on `muster/issue-42-fix`, added as a project; its
/// checkout's id.
pub(crate) async fn project(dir: &Path, supervisor: &SupervisorHandle) -> (PathBuf, WorkspaceId) {
    let root = repository(dir, "app");
    git(&root, &["checkout", "--quiet", "-b", "muster/issue-42-fix"]);
    supervisor
        .command(Command::AddProject {
            root: root.clone(),
            name: None,
        })
        .await
        .unwrap();
    let snapshot = state(supervisor).await;
    let checkout = snapshot.workspaces[0].id;
    (root, checkout)
}

pub(crate) async fn state(supervisor: &SupervisorHandle) -> Snapshot {
    supervisor.subscribe().await.unwrap().0
}

/// A panel that writes where it runs and what it is told into `out`,
/// then waits.
fn telling(out: &Path) -> PanelLaunch {
    let script = r#"pwd > "$OUT/pwd"; echo $$ > "$OUT/pid"; env > "$OUT/env.part"; mv "$OUT/env.part" "$OUT/env"; exec sleep 60"#;
    let mut panel = PanelLaunch::new(
        "demo",
        "context",
        "context",
        ["/bin/sh", "-c", script].map(str::to_owned).to_vec(),
    );
    panel.env = vec![("OUT".to_owned(), out.display().to_string())];
    panel
}

/// What the panel writing into `out` was told, once it has written it.
async fn told(out: &Path) -> HashMap<String, String> {
    let env = waiting::found("the panel to run", async || {
        std::fs::read_to_string(out.join("env")).ok()
    })
    .await;
    env.lines()
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.to_owned(), value.to_owned()))
        .collect()
}

pub(crate) fn started(reply: Reply) -> SessionId {
    match reply {
        Reply::SessionStarted { id } => id,
        other => panic!("unexpected {other:?}"),
    }
}

pub(crate) fn session(snapshot: &Snapshot, id: SessionId) -> &Session {
    snapshot
        .sessions
        .iter()
        .find(|found| found.id == id)
        .unwrap()
}

/// Waits until `done` holds of the state.
pub(crate) async fn until(
    supervisor: &SupervisorHandle,
    done: impl Fn(&Snapshot) -> bool,
) -> Snapshot {
    waiting::found("the state to get there", async || {
        Some(state(supervisor).await).filter(|snapshot| done(snapshot))
    })
    .await
}

#[tokio::test]
async fn a_panel_runs_in_its_workspace_told_what_muster_knows_and_is_no_news_prd_15() {
    let dir = tempfile::tempdir().unwrap();
    let first = spawn(dir.path(), Vec::new()).await;
    let (root, checkout) = project(dir.path(), &first).await;
    // A Claude Code chat that ran a turn, as a restart finds it.
    let chat = Session {
        id: SessionId::new(),
        workspace: checkout,
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
        started_by: None,
        agent: None,
    };
    let mut store = Store::open(&dir.path().join("muster.db"), FixedClock.now()).unwrap();
    store.insert_session(&chat).unwrap();
    let transcript = dir.path().join("claude/projects/-app/c1.jsonl");
    std::fs::create_dir_all(transcript.parent().unwrap()).unwrap();
    std::fs::write(&transcript, "{}\n").unwrap();
    let supervisor = spawn(dir.path(), Vec::new()).await;

    let out = tempfile::tempdir().unwrap();
    let panel = started(
        supervisor
            .start_panel(checkout, Some(chat.id), telling(out.path()))
            .await
            .unwrap(),
    );
    let env = told(out.path()).await;
    let pwd = std::fs::read_to_string(out.path().join("pwd")).unwrap();
    assert_eq!(Path::new(pwd.trim()).canonicalize().unwrap(), root);
    let value = |key: &str| env.get(key).map(String::as_str);
    let root_text = root.display().to_string();
    let transcript_text = transcript.display().to_string();
    let chat_id = chat.id.to_string();
    let panel_id = panel.to_string();
    for (key, wanted) in [
        ("MUSTER_PLUGIN_PROJECT", Some("app")),
        ("MUSTER_PLUGIN_ROOT", Some(root_text.as_str())),
        ("MUSTER_PLUGIN_WORKSPACE", Some(root_text.as_str())),
        ("MUSTER_PLUGIN_BRANCH", Some("muster/issue-42-fix")),
        ("MUSTER_PLUGIN_SESSION", Some(chat_id.as_str())),
        ("MUSTER_PLUGIN_HARNESS", Some("claude")),
        ("MUSTER_PLUGIN_RESUME", Some("c1")),
        ("MUSTER_PLUGIN_TRANSCRIPT", Some(transcript_text.as_str())),
        ("MUSTER_PLUGIN_TICKET", Some("42")),
        // No pull request from the branch yet.
        ("MUSTER_PLUGIN_PR", None),
        // Its plugin's own, and the terminal every session is told of.
        ("OUT", Some(out.path().to_str().unwrap())),
        ("TERM_PROGRAM", Some("muster")),
        ("MUSTER_SESSION", Some(panel_id.as_str())),
        // No agent's: no hooks, and nothing of an agent the daemon was
        // started from.
        ("MUSTER_HOOK_TOKEN", None),
        ("MUSTER_HOOK_SOCKET", None),
        ("CLAUDECODE", None),
        ("CLAUDE_CODE_ENTRYPOINT", None),
    ] {
        assert_eq!(value(key), wanted, "{key}");
    }

    let snapshot = state(&supervisor).await;
    let stored = session(&snapshot, panel);
    assert_eq!(stored.harness, "plugin:demo/context");
    assert_eq!(stored.title, "context");
    assert_eq!(stored.resume_id, None);
    assert!(!muster_core::news::needs_attention(stored));
    // No shell, named by its foreground program.
    let shells = supervisor.terminals().await.unwrap().list;
    assert!(shells.iter().all(|(id, _)| *id != panel));

    // Beside a session of its own workspace only; and with a program.
    let other = SessionId::new();
    let refused = supervisor
        .start_panel(checkout, Some(other), telling(out.path()))
        .await
        .unwrap_err();
    assert_eq!(refused.code, ErrorCode::NotFound, "{refused}");
    let nothing = PanelLaunch::new("demo", "empty", "empty", Vec::new());
    let refused = supervisor
        .start_panel(checkout, None, nothing)
        .await
        .unwrap_err();
    assert_eq!(refused.code, ErrorCode::BadRequest);
    assert!(refused.message.contains("names no program"), "{refused}");
    supervisor
        .command(Command::StopSession { session: panel })
        .await
        .unwrap();
}

/// A worktree `feature` of the project at `root`, and its id.
async fn worktree(supervisor: &SupervisorHandle) -> WorkspaceId {
    let project = state(supervisor).await.projects[0].id;
    match supervisor
        .command(Command::CreateWorktree {
            project,
            branch: Some("feature".to_owned()),
            on: None,
            named_for: None,
        })
        .await
        .unwrap()
    {
        Reply::WorktreeCreated { id } => id,
        other => panic!("unexpected {other:?}"),
    }
}

fn alive(pid: &str) -> bool {
    std::process::Command::new("kill")
        .args(["-0", pid.trim()])
        .status()
        .unwrap()
        .success()
}

/// The pid the panel writing into `out` wrote.
async fn pid(out: &Path) -> String {
    told(out).await;
    std::fs::read_to_string(out.join("pid")).unwrap()
}

#[tokio::test]
async fn a_panel_ends_with_its_worktree_as_a_shell_does() {
    let dir = tempfile::tempdir().unwrap();
    let supervisor = spawn(dir.path(), Vec::new()).await;
    project(dir.path(), &supervisor).await;
    let feature = worktree(&supervisor).await;
    let out = tempfile::tempdir().unwrap();
    let panel = started(
        supervisor
            .start_panel(feature, None, telling(out.path()))
            .await
            .unwrap(),
    );
    let pid = pid(out.path()).await;
    // Beside no session: told of the workspace alone.
    let env = told(out.path()).await;
    assert_eq!(
        env.get("MUSTER_PLUGIN_BRANCH").map(String::as_str),
        Some("feature")
    );
    assert!(!env.contains_key("MUSTER_PLUGIN_SESSION"));

    supervisor
        .command(Command::RemoveWorktree {
            workspace: feature,
            force: false,
        })
        .await
        .unwrap();
    until(&supervisor, |snapshot| {
        snapshot
            .sessions
            .iter()
            .all(|found| found.id != panel || found.ended_at.is_some())
    })
    .await;
    waiting::until("its program to stop", async || !alive(&pid)).await;
}

#[tokio::test]
async fn a_panel_runs_on_through_a_restart_in_place_and_comes_back_a_panel() {
    let dir = tempfile::tempdir().unwrap();
    let before = spawn(dir.path(), Vec::new()).await;
    let (_, checkout) = project(dir.path(), &before).await;
    let out = tempfile::tempdir().unwrap();
    let panel = started(
        before
            .start_panel(checkout, None, telling(out.path()))
            .await
            .unwrap(),
    );
    let pid = pid(out.path()).await;

    let handed = before.hand_over().await;
    assert_eq!(handed.len(), 1);
    let after = spawn(dir.path(), handed).await;
    let snapshot = state(&after).await;
    let stored = session(&snapshot, panel);
    assert_eq!(stored.harness, "plugin:demo/context");
    assert_eq!(stored.ended_at, None, "still running");
    let running = after.running_processes().await.unwrap();
    assert!(running.iter().any(|(id, _)| *id == panel), "{running:?}");
    assert!(alive(&pid), "the same program, never stopped");
    // Taken back as a panel: no shell, named by its foreground program.
    let shells = after.terminals().await.unwrap().list;
    assert!(shells.iter().all(|(id, _)| *id != panel));
    after
        .command(Command::StopSession { session: panel })
        .await
        .unwrap();
}

#[tokio::test]
async fn a_daemon_that_crashed_ends_a_panel_as_it_ends_a_shell() {
    let dir = tempfile::tempdir().unwrap();
    let crashed = spawn(dir.path(), Vec::new()).await;
    let (_, checkout) = project(dir.path(), &crashed).await;
    let out = tempfile::tempdir().unwrap();
    let panel = started(
        crashed
            .start_panel(checkout, None, telling(out.path()))
            .await
            .unwrap(),
    );
    told(out.path()).await;

    // The next daemon finds it running in the store, with nothing handed
    // over: it cannot resume, so it ended.
    let next = spawn(dir.path(), Vec::new()).await;
    let snapshot = until(&next, |snapshot| {
        snapshot
            .sessions
            .iter()
            .any(|found| found.id == panel && found.ended_at.is_some())
    })
    .await;
    assert!(!muster_core::news::needs_attention(session(
        &snapshot, panel
    )));
    crashed
        .command(Command::StopSession { session: panel })
        .await
        .unwrap();
}

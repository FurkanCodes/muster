//! End-to-end tests for worktrees made or removed outside Muster, by hand
//! or by an agent: they appear and go as git says, without undoing what
//! Muster itself does.

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

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use muster_client::Connection;
use muster_core::protocol::{Command, Reply, Snapshot, TermSize};
use muster_core::{ProjectId, SessionId, Workspace, WorkspaceId};

use support::{Sandbox, git};

async fn add_project(connection: &mut Connection, root: &Path) -> ProjectId {
    match connection
        .command(Command::AddProject {
            root: root.to_owned(),
            name: None,
        })
        .await
        .unwrap()
    {
        Reply::ProjectAdded { id } => id,
        other => panic!("unexpected {other:?}"),
    }
}

fn worktrees(snapshot: &Snapshot) -> Vec<&Workspace> {
    snapshot
        .workspaces
        .iter()
        .filter(|workspace| !workspace.kind.is_checkout())
        .collect()
}

/// Waits until the daemon's state satisfies `done`.
async fn wait_until(connection: &mut Connection, done: impl Fn(&Snapshot) -> bool) -> Snapshot {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let snapshot = connection.subscribe().await.unwrap();
        if done(&snapshot) {
            return snapshot;
        }
        assert!(Instant::now() < deadline, "{:#?}", snapshot.workspaces);
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
}

fn fast() -> Sandbox {
    Sandbox::new().with_env("MUSTER_WORKTREE_MS", "50")
}

fn path_of(snapshot: &Snapshot, id: WorkspaceId) -> PathBuf {
    snapshot
        .workspaces
        .iter()
        .find(|workspace| workspace.id == id)
        .unwrap()
        .kind
        .host_path()
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn a_worktree_made_with_git_appears_and_goes_when_git_removes_it() {
    let sandbox = fast();
    let root = sandbox.repository("app");
    let daemon = sandbox.start();
    let mut connection = daemon.connect().await;
    add_project(&mut connection, &root).await;

    let outside = root.parent().unwrap().join("by hand");
    git(
        &root,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "manual",
            outside.to_str().unwrap(),
        ],
    );
    let outside = outside.canonicalize().unwrap();
    let snapshot = wait_until(&mut connection, |s| worktrees(s).len() == 1).await;
    let adopted = worktrees(&snapshot)[0].clone();
    assert_eq!(adopted.kind.host_path().unwrap(), outside);
    assert_eq!(adopted.kind.branch(), "manual");

    // A session can run in it.
    let reply = connection
        .command(Command::StartSession {
            workspace: adopted.id,
            harness: "shell".to_owned(),
            title: None,
            size: TermSize::default(),
            prompt: None,
            started_by: None,
        })
        .await
        .unwrap();
    let Reply::SessionStarted { id: session } = reply else {
        panic!("{reply:?}");
    };
    connection
        .command(Command::StopSession { session })
        .await
        .unwrap();
    wait_until(&mut connection, |s| {
        s.sessions
            .iter()
            .any(|x| x.id == session && x.ended_at.is_some())
    })
    .await;

    git(
        &root,
        &["worktree", "remove", "--force", outside.to_str().unwrap()],
    );
    let snapshot = wait_until(&mut connection, |s| worktrees(s).is_empty()).await;
    assert!(snapshot.sessions.is_empty(), "its sessions went with it");
}

#[tokio::test]
async fn a_worktree_deleted_by_hand_is_forgotten_once_nothing_runs_in_it() {
    let sandbox = fast();
    let root = sandbox.repository("app");
    let daemon = sandbox.start();
    let mut connection = daemon.connect().await;
    let project = add_project(&mut connection, &root).await;
    let Reply::WorktreeCreated { id } = connection
        .command(Command::CreateWorktree {
            project,
            branch: Some("doomed".to_owned()),
            on: None,
            named_for: None,
        })
        .await
        .unwrap()
    else {
        panic!("expected WorktreeCreated");
    };
    let snapshot = connection.subscribe().await.unwrap();
    let path = path_of(&snapshot, id);
    let reply = connection
        .command(Command::StartSession {
            workspace: id,
            harness: "shell".to_owned(),
            title: None,
            size: TermSize::default(),
            prompt: None,
            started_by: None,
        })
        .await
        .unwrap();
    let Reply::SessionStarted { id: session } = reply else {
        panic!("{reply:?}");
    };

    std::fs::remove_dir_all(&path).unwrap();
    // Kept while its session runs.
    tokio::time::sleep(Duration::from_millis(400)).await;
    let snapshot = connection.subscribe().await.unwrap();
    assert!(snapshot.workspaces.iter().any(|w| w.id == id));

    stop_and_wait(&mut connection, session).await;
    wait_until(&mut connection, |s| {
        !s.workspaces.iter().any(|w| w.id == id)
    })
    .await;
    // The branch is git's and stays.
    assert_eq!(git(&root, &["branch", "--list", "doomed"]), "doomed");
}

async fn stop_and_wait(connection: &mut Connection, session: SessionId) {
    connection
        .command(Command::StopSession { session })
        .await
        .unwrap();
    wait_until(connection, |s| {
        s.sessions
            .iter()
            .all(|x| x.id != session || x.ended_at.is_some())
    })
    .await;
}

#[tokio::test]
async fn muster_s_own_worktrees_are_neither_doubled_nor_brought_back() {
    let sandbox = Sandbox::new().with_env("MUSTER_WORKTREE_MS", "10");
    let root = sandbox.repository("app");
    let daemon = sandbox.start();
    let mut connection = daemon.connect().await;
    let project = add_project(&mut connection, &root).await;
    for round in 0..8 {
        let Reply::WorktreeCreated { id } = connection
            .command(Command::CreateWorktree {
                project,
                branch: Some(format!("race-{round}")),
                on: None,
                named_for: None,
            })
            .await
            .unwrap()
        else {
            panic!("expected WorktreeCreated");
        };
        tokio::time::sleep(Duration::from_millis(15)).await;
        if round % 2 == 0 {
            connection
                .command(Command::RemoveWorktree {
                    workspace: id,
                    force: false,
                })
                .await
                .unwrap();
        }
    }
    // Let the sync look several times more.
    tokio::time::sleep(Duration::from_millis(300)).await;
    let snapshot = connection.subscribe().await.unwrap();
    let mut branches: Vec<_> = worktrees(&snapshot)
        .iter()
        .map(|w| w.kind.branch().to_owned())
        .collect();
    branches.sort();
    assert_eq!(branches, ["race-1", "race-3", "race-5", "race-7"]);
}

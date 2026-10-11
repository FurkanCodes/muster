//! In-process tests of the Supervisor and the delta pump. The end-to-end tests
//! against the real binary live in `crates/muster/tests`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use muster_core::Timestamp;
use muster_core::protocol::{Command, Delta, ErrorCode, Reply, ServerFrame};
use tokio::sync::mpsc;
use tokio::time::timeout;

use crate::clock::Clock;
use crate::server::pump_state;
use crate::store::{Store, StoreHandle};
use crate::supervisor::{self, SupervisorHandle};

#[derive(Debug)]
struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> Timestamp {
        Timestamp::from_millis(42)
    }
}

async fn start(delta_capacity: usize) -> SupervisorHandle {
    let store = StoreHandle::spawn(Store::open_in_memory().unwrap()).unwrap();
    supervisor::spawn(supervisor::Settings {
        store,
        clock: Arc::new(FixedClock),
        worktrees_dir: std::env::temp_dir().join("muster-unit-worktrees"),
        delta_capacity,
    })
    .await
    .unwrap()
}

/// Makes a directory that looks like a git repository.
fn repository(parent: &Path, name: &str) -> PathBuf {
    let root = parent.join(name);
    std::fs::create_dir_all(root.join(".git")).unwrap();
    root.canonicalize().unwrap()
}

fn add(root: &Path, name: Option<&str>) -> Command {
    Command::AddProject {
        root: root.to_owned(),
        name: name.map(str::to_owned),
    }
}

async fn refused(supervisor: &SupervisorHandle, command: Command) -> ErrorCode {
    supervisor.command(command).await.unwrap_err().code
}

#[tokio::test]
async fn an_added_project_is_broadcast_after_the_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let root = repository(dir.path(), "app");
    let supervisor = start(16).await;
    let (snapshot, mut deltas) = supervisor.subscribe().await.unwrap();
    assert!(snapshot.projects.is_empty());

    let Reply::ProjectAdded { id } = supervisor.command(add(&root, None)).await.unwrap() else {
        panic!("expected ProjectAdded");
    };
    let Delta::ProjectUpserted(project) = deltas.recv().await.unwrap() else {
        panic!("expected ProjectUpserted");
    };
    assert_eq!((project.id, project.name.as_str()), (id, "app"));
    assert_eq!(project.root, root);
    assert_eq!(project.created_at, Timestamp::from_millis(42));

    let (snapshot, _) = supervisor.subscribe().await.unwrap();
    assert_eq!(snapshot.projects, [project]);
}

#[tokio::test]
async fn a_project_can_be_named_and_a_blank_name_is_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let supervisor = start(16).await;
    supervisor
        .command(add(&repository(dir.path(), "one"), Some("Lamphaus")))
        .await
        .unwrap();
    supervisor
        .command(add(&repository(dir.path(), "two"), Some("  ")))
        .await
        .unwrap();
    let (snapshot, _) = supervisor.subscribe().await.unwrap();
    let names: Vec<_> = snapshot.projects.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["Lamphaus", "two"]);
}

#[tokio::test]
async fn only_absolute_paths_to_git_repositories_are_projects() {
    let dir = tempfile::tempdir().unwrap();
    let supervisor = start(16).await;
    let plain = dir.path().join("plain");
    std::fs::create_dir(&plain).unwrap();

    assert_eq!(
        refused(&supervisor, add(Path::new("relative/app"), None)).await,
        ErrorCode::BadRequest
    );
    assert_eq!(
        refused(&supervisor, add(&dir.path().join("missing"), None)).await,
        ErrorCode::BadRequest
    );
    assert_eq!(
        refused(&supervisor, add(&plain, None)).await,
        ErrorCode::BadRequest
    );
}

#[tokio::test]
async fn the_same_repository_by_another_path_conflicts() {
    let dir = tempfile::tempdir().unwrap();
    let root = repository(dir.path(), "app");
    let link = dir.path().join("link");
    std::os::unix::fs::symlink(&root, &link).unwrap();
    let supervisor = start(16).await;

    supervisor.command(add(&root, None)).await.unwrap();
    assert_eq!(
        refused(&supervisor, add(&link, None)).await,
        ErrorCode::Conflict
    );
}

#[tokio::test]
async fn a_subscriber_that_falls_behind_gets_a_fresh_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let supervisor = start(2).await;
    // Room for one frame: the pump blocks on the second while the test is not reading.
    let (outgoing, mut frames) = mpsc::channel(1);
    tokio::spawn(pump_state(supervisor.clone(), outgoing));
    assert!(matches!(
        frames.recv().await,
        Some(ServerFrame::Snapshot(_))
    ));

    for index in 0..6 {
        let root = repository(dir.path(), &format!("app{index}"));
        supervisor.command(add(&root, None)).await.unwrap();
    }

    let resent = timeout(Duration::from_secs(5), async {
        loop {
            match frames.recv().await {
                Some(ServerFrame::Snapshot(snapshot)) => return snapshot,
                Some(ServerFrame::Delta(_)) => continue,
                other => panic!("unexpected {other:?}"),
            }
        }
    })
    .await
    .expect("a fresh snapshot");
    assert_eq!(resent.projects.len(), 6);
}

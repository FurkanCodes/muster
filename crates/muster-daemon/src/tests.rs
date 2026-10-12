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
        hook_socket: std::env::temp_dir().join("muster-unit-hooks.sock"),
        hook_queue: tokio::sync::mpsc::channel(1).1,
        delta_capacity,
        history_every: 1,
        timing: muster_state::session::Timing::default(),
        work: std::sync::Arc::new(crate::runs::NotYetBuilt),
        trackers: std::sync::Arc::new(crate::trackers::choose::ServiceTrackers {
            secrets: std::sync::Arc::new(crate::secrets::Kept::default()),
            http: crate::service::http_client(),
        }),
        config_dir: std::env::temp_dir(),
        credentials: Arc::new(crate::runs::credentials::FixedCredentials::default()),
        docker: "docker".to_owned(),
        hosts: Arc::new(crate::githost::choose::ServiceHosts {
            secrets: Arc::new(crate::secrets::Kept::default()),
            http: crate::service::http_client(),
        }),
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
async fn any_folder_named_by_an_absolute_path_is_a_project() {
    let dir = tempfile::tempdir().unwrap();
    let supervisor = start(16).await;
    let file = dir.path().join("notes.txt");
    std::fs::write(&file, "text").unwrap();

    assert_eq!(
        refused(&supervisor, add(Path::new("relative/app"), None)).await,
        ErrorCode::BadRequest
    );
    assert_eq!(
        refused(&supervisor, add(&dir.path().join("missing"), None)).await,
        ErrorCode::BadRequest
    );
    assert_eq!(
        refused(&supervisor, add(&file, None)).await,
        ErrorCode::BadRequest
    );

    // A plain folder, not a git repository: its checkout has no branch.
    let plain = dir.path().join("plain");
    std::fs::create_dir(&plain).unwrap();
    let Reply::ProjectAdded { id } = supervisor.command(add(&plain, None)).await.unwrap() else {
        panic!("expected ProjectAdded");
    };
    let (snapshot, _) = supervisor.subscribe().await.unwrap();
    assert_eq!(snapshot.projects[0].name, "plain");
    let checkout = &snapshot.workspaces[0];
    assert_eq!(checkout.project, id);
    assert!(checkout.kind.is_checkout() && !checkout.kind.in_git());
    assert_eq!(
        checkout.kind.host_path(),
        Some(plain.canonicalize().unwrap().as_path())
    );
}

#[tokio::test]
async fn what_needs_git_is_refused_plainly_in_a_plain_folder() {
    let dir = tempfile::tempdir().unwrap();
    let plain = dir.path().join("notes");
    std::fs::create_dir(&plain).unwrap();
    let supervisor = start(16).await;
    let Reply::ProjectAdded { id: project } = supervisor.command(add(&plain, None)).await.unwrap()
    else {
        panic!("expected ProjectAdded");
    };
    let (snapshot, _) = supervisor.subscribe().await.unwrap();
    let checkout = snapshot.workspaces[0].id;

    for (command, says) in [
        (
            Command::CreateWorktree {
                project,
                branch: Some("work".to_owned()),
            },
            "notes is a plain folder, not a git repository; worktrees need one",
        ),
        (
            Command::ListIssues { project },
            "this project has no git remote to find its tickets by; say where its tickets are",
        ),
        (
            Command::ViewIssue {
                project,
                key: muster_core::TicketKey::number(1),
            },
            "say where its tickets are with [tickets] tracker",
        ),
        (
            Command::CloseIssue {
                project,
                key: muster_core::TicketKey::number(1),
            },
            "say where its tickets are with [tickets] tracker",
        ),
        (
            Command::StartFromIssue {
                project,
                key: muster_core::TicketKey::number(1),
                harness: "shell".to_owned(),
                size: Default::default(),
                another: false,
            },
            "notes is a plain folder, not a git repository; worktrees for tickets need one",
        ),
        (
            Command::StartRun {
                project,
                number: 1,
                from: None,
            },
            "runs need one",
        ),
        (
            Command::OpenPullRequest {
                workspace: checkout,
            },
            "it has no branch to review or push",
        ),
        (
            Command::ReviewWorkspace {
                workspace: checkout,
                size: Default::default(),
            },
            "it has no branch to review or push",
        ),
    ] {
        let error = supervisor.command(command.clone()).await.unwrap_err();
        assert_eq!(error.code, ErrorCode::BadRequest, "{command:?}: {error}");
        assert!(error.message.contains(says), "{command:?}: {error}");
    }
    assert!(!plain.join(".git").exists(), "nothing made it a repository");
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

#[tokio::test]
async fn every_nth_measurement_is_kept_in_history() {
    use muster_core::protocol::{ResourceReport, SessionUsage};
    use muster_core::{
        Project, ProjectId, Session, SessionId, SessionStatus, Workspace, WorkspaceId,
        WorkspaceKind,
    };

    let store = StoreHandle::spawn(Store::open_in_memory().unwrap()).unwrap();
    let project = Project {
        id: ProjectId::new(),
        name: "app".to_owned(),
        root: PathBuf::from("/src/app"),
        created_at: Timestamp::from_millis(1),
    };
    let workspace = Workspace {
        id: WorkspaceId::new(),
        project: project.id,
        kind: WorkspaceKind::Worktree {
            path: PathBuf::from("/w/work"),
            branch: "work".to_owned(),
        },
        created_at: Timestamp::from_millis(1),
    };
    let session = Session {
        id: SessionId::new(),
        workspace: workspace.id,
        harness: "shell".to_owned(),
        title: "Shell".to_owned(),
        status: SessionStatus::Fresh,
        unseen: false,
        resume_id: None,
        created_at: Timestamp::from_millis(1),
        ended_at: None,
    };
    let id = session.id;
    store
        .call(move |store| {
            store.insert_project(&project)?;
            store.insert_workspace(&workspace)?;
            store.insert_session(&session)
        })
        .await
        .unwrap()
        .unwrap();
    let supervisor = supervisor::spawn(supervisor::Settings {
        store: store.clone(),
        clock: Arc::new(FixedClock),
        worktrees_dir: std::env::temp_dir().join("muster-unit-worktrees"),
        hook_socket: std::env::temp_dir().join("muster-unit-hooks.sock"),
        hook_queue: tokio::sync::mpsc::channel(1).1,
        delta_capacity: 16,
        history_every: 2,
        timing: muster_state::session::Timing::default(),
        work: std::sync::Arc::new(crate::runs::NotYetBuilt),
        trackers: std::sync::Arc::new(crate::trackers::choose::ServiceTrackers {
            secrets: std::sync::Arc::new(crate::secrets::Kept::default()),
            http: crate::service::http_client(),
        }),
        config_dir: std::env::temp_dir(),
        credentials: Arc::new(crate::runs::credentials::FixedCredentials::default()),
        docker: "docker".to_owned(),
        hosts: Arc::new(crate::githost::choose::ServiceHosts {
            secrets: Arc::new(crate::secrets::Kept::default()),
            http: crate::service::http_client(),
        }),
    })
    .await
    .unwrap();

    let report = ResourceReport {
        host_memory_used: 1,
        host_memory_total: 2,
        sessions: vec![SessionUsage {
            session: id,
            memory_bytes: 1_000,
            cpu_permille: 5,
        }],
    };
    let kept = || async {
        // A round trip, so the measurements before it have been handled.
        supervisor.subscribe().await.unwrap();
        store
            .call(move |store| store.samples(id, Timestamp::from_millis(0)))
            .await
            .unwrap()
            .unwrap()
    };
    assert!(supervisor.record_resources(report.clone()).await);
    assert!(kept().await.is_empty(), "the first is not kept");
    assert!(supervisor.record_resources(report).await);
    let samples = kept().await;
    assert_eq!(samples.len(), 1);
    assert_eq!(samples[0].memory_bytes, 1_000);
}

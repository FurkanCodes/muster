//! End-to-end tests for worktrees (M1 step 6), against the real binary and
//! real git repositories.

// Helpers outside `#[test]` functions are not covered by clippy.toml's test exemption.
#![allow(clippy::unwrap_used)]

mod support;

use std::path::PathBuf;

use muster_client::{ClientError, Connection};
use muster_core::protocol::{Command, Delta, ErrorCode, Reply, ServerFrame};
use muster_core::{ProjectId, WorkspaceId, WorkspaceKind};

use support::{Sandbox, expect_event, git};

async fn add_project(connection: &mut Connection, root: &std::path::Path) -> ProjectId {
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

async fn create(
    connection: &mut Connection,
    project: ProjectId,
    branch: Option<&str>,
) -> Result<WorkspaceId, ClientError> {
    connection
        .command(Command::CreateWorktree {
            project,
            branch: branch.map(str::to_owned),
        })
        .await
        .map(|reply| match reply {
            Reply::WorktreeCreated { id } => id,
            other => panic!("unexpected {other:?}"),
        })
}

/// The path and branch of worktree `id`, from a fresh snapshot.
async fn worktree(connection: &mut Connection, id: WorkspaceId) -> (PathBuf, String) {
    let snapshot = connection.subscribe().await.unwrap();
    let workspace = snapshot
        .workspaces
        .into_iter()
        .find(|workspace| workspace.id == id)
        .expect("the worktree is in the snapshot");
    let WorkspaceKind::Worktree { path, branch } = workspace.kind;
    (path, branch)
}

fn code(result: Result<impl std::fmt::Debug, ClientError>) -> ErrorCode {
    match result {
        Err(ClientError::Daemon(error)) => error.code,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[tokio::test]
async fn a_worktree_is_created_on_a_new_branch_and_announced() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    let daemon = sandbox.start();
    let mut watcher = daemon.connect().await;
    watcher.subscribe().await.unwrap();
    let mut connection = daemon.connect().await;
    let project = add_project(&mut connection, &root).await;

    let id = create(&mut connection, project, Some("feature/login"))
        .await
        .unwrap();
    let announced = expect_event(&mut watcher, |frame| match frame {
        ServerFrame::Delta(Delta::WorkspaceUpserted(workspace)) => Some(workspace.clone()),
        _ => None,
    })
    .await;
    assert_eq!(announced.id, id);

    let (path, branch) = worktree(&mut connection, id).await;
    assert_eq!(branch, "feature/login");
    let worktrees_dir = sandbox.paths().worktrees_dir.canonicalize().unwrap();
    assert_eq!(path, worktrees_dir.join("app/feature-login"));
    assert!(path.join("README.md").exists());
    assert!(git(&root, &["worktree", "list"]).contains(path.to_str().unwrap()));
    assert_eq!(git(&path, &["branch", "--show-current"]), "feature/login");
}

#[tokio::test]
async fn without_a_branch_a_namespaced_one_is_made() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    let daemon = sandbox.start();
    let mut connection = daemon.connect().await;
    let project = add_project(&mut connection, &root).await;

    let id = create(&mut connection, project, None).await.unwrap();
    let (_, branch) = worktree(&mut connection, id).await;
    assert!(branch.starts_with("muster/"), "{branch}");
}

#[tokio::test]
async fn an_existing_branch_is_checked_out_not_recreated() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    git(&root, &["branch", "existing"]);
    let tip = git(&root, &["rev-parse", "existing"]);
    let daemon = sandbox.start();
    let mut connection = daemon.connect().await;
    let project = add_project(&mut connection, &root).await;

    let id = create(&mut connection, project, Some("existing"))
        .await
        .unwrap();
    let (path, _) = worktree(&mut connection, id).await;
    assert_eq!(git(&path, &["rev-parse", "HEAD"]), tip);
}

#[tokio::test]
async fn bad_requests_are_refused_with_their_code() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    let daemon = sandbox.start();
    let mut connection = daemon.connect().await;
    let project = add_project(&mut connection, &root).await;

    assert_eq!(
        code(create(&mut connection, project, Some("bad..name")).await),
        ErrorCode::BadRequest
    );
    assert_eq!(
        code(create(&mut connection, ProjectId::new(), None).await),
        ErrorCode::NotFound
    );
    // The main checkout already has `main`.
    assert_eq!(
        code(create(&mut connection, project, Some("main")).await),
        ErrorCode::Conflict
    );

    create(&mut connection, project, Some("twice"))
        .await
        .unwrap();
    assert_eq!(
        code(create(&mut connection, project, Some("twice")).await),
        ErrorCode::Conflict
    );

    // A branch name that would also be another branch's folder, both ways.
    git(&root, &["branch", "nest/inner"]);
    let clash = create(&mut connection, project, Some("nest"))
        .await
        .unwrap_err();
    let ClientError::Daemon(clash) = clash else {
        panic!("expected a refusal");
    };
    assert_eq!(clash.code, ErrorCode::Conflict);
    assert!(
        clash.message.contains("the branch nest/inner exists"),
        "{}",
        clash.message
    );
    let clash = create(&mut connection, project, Some("main/x"))
        .await
        .unwrap_err();
    let ClientError::Daemon(clash) = clash else {
        panic!("expected a refusal");
    };
    assert!(
        clash.message.contains("the branch main exists"),
        "{}",
        clash.message
    );

    // Refusals are logged with their reason, for reading afterwards.
    let log = daemon.log();
    assert!(log.contains("command refused"), "{log}");
    assert!(log.contains("not a valid branch name"), "{log}");
}

#[tokio::test]
async fn removing_keeps_the_branch_and_protects_uncommitted_work() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    let daemon = sandbox.start();
    let mut watcher = daemon.connect().await;
    watcher.subscribe().await.unwrap();
    let mut connection = daemon.connect().await;
    let project = add_project(&mut connection, &root).await;
    let id = create(&mut connection, project, Some("work"))
        .await
        .unwrap();
    let (path, _) = worktree(&mut connection, id).await;

    std::fs::write(path.join("README.md"), "changed\n").unwrap();
    let remove = |force| Command::RemoveWorktree {
        workspace: id,
        force,
    };
    assert_eq!(
        code(connection.command(remove(false)).await),
        ErrorCode::Conflict
    );
    assert!(path.exists());

    assert_eq!(connection.command(remove(true)).await.unwrap(), Reply::Done);
    expect_event(&mut watcher, |frame| match frame {
        ServerFrame::Delta(Delta::WorkspaceRemoved { id: removed }) if *removed == id => Some(()),
        _ => None,
    })
    .await;
    assert!(!path.exists());
    assert_eq!(git(&root, &["branch", "--list", "work"]), "work");
    assert!(connection.subscribe().await.unwrap().workspaces.is_empty());
    assert_eq!(
        code(connection.command(remove(false)).await),
        ErrorCode::NotFound
    );
}

#[tokio::test]
async fn a_worktree_deleted_by_hand_can_still_be_removed() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    let daemon = sandbox.start();
    let mut connection = daemon.connect().await;
    let project = add_project(&mut connection, &root).await;
    let id = create(&mut connection, project, Some("gone"))
        .await
        .unwrap();
    let (path, _) = worktree(&mut connection, id).await;

    std::fs::remove_dir_all(&path).unwrap();
    let removed = connection
        .command(Command::RemoveWorktree {
            workspace: id,
            force: false,
        })
        .await;
    assert_eq!(removed.unwrap(), Reply::Done);
    assert!(!git(&root, &["worktree", "list"]).contains("gone"));
}

#[tokio::test]
async fn worktrees_survive_a_daemon_restart() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    let daemon = sandbox.start();
    let mut connection = daemon.connect().await;
    let project = add_project(&mut connection, &root).await;
    let id = create(&mut connection, project, Some("kept"))
        .await
        .unwrap();
    drop(connection);
    assert!(daemon.stop().success());

    let daemon = sandbox.start();
    let (_, branch) = worktree(&mut daemon.connect().await, id).await;
    assert_eq!(branch, "kept");
}

#[test]
fn the_command_line_manages_worktrees_and_sessions() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    let _daemon = sandbox.start();
    let run = |args: &[&str]| {
        let output = sandbox.run(args);
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        (output.status.success(), stdout, stderr)
    };

    assert!(run(&["project", "add", root.to_str().unwrap()]).0);
    let (ok, stdout, stderr) = run(&["worktree", "add", "app", "--branch", "cli-work"]);
    assert!(ok, "{stderr}");
    assert!(stdout.contains("Created cli-work at"), "{stdout}");

    let (ok, stdout, stderr) = run(&["session", "start", "cli-work"]);
    assert!(ok, "{stderr}");
    let session = stdout
        .trim()
        .strip_prefix("Started ")
        .and_then(|rest| rest.strip_suffix('.'))
        .expect("the session id")
        .to_owned();

    let (_, status, _) = run(&["status"]);
    assert!(
        status.contains("1 project, 1 worktree, 1 session"),
        "{status}"
    );
    assert!(status.contains("cli-work"), "{status}");
    assert!(
        status.contains(&format!("{session}  Shell  fresh")),
        "{status}"
    );

    let (ok, _, stderr) = run(&["worktree", "remove", "cli-work"]);
    assert!(!ok);
    assert!(stderr.contains("running session"), "{stderr}");

    assert!(run(&["session", "stop", &session]).0);
    // The stop finishes asynchronously; removal succeeds once it has.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let (ok, stdout, stderr) = run(&["worktree", "remove", "cli-work"]);
        if ok {
            assert!(stdout.contains("its branch is kept"), "{stdout}");
            break;
        }
        assert!(std::time::Instant::now() < deadline, "{stderr}");
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(run(&["status"]).1.contains("0 worktrees"));
}

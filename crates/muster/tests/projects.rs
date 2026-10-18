//! End-to-end tests for projects: each one's own checkout as a workspace,
//! branches kept current as they change under Muster, and removing a
//! project without touching its repository.

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

use std::path::PathBuf;
use std::time::Duration;

use muster_client::{ClientError, Connection};
use muster_core::protocol::{Command, Delta, ErrorCode, Reply, ServerFrame, Snapshot, TermSize};
use muster_core::{Project, ProjectId, SessionId, Timestamp, Workspace, WorkspaceId};

use support::{Sandbox, expect_event, git, read_terminal};

async fn add_project(connection: &mut Connection, root: PathBuf) -> ProjectId {
    match connection
        .command(Command::AddProject { root, name: None })
        .await
        .unwrap()
    {
        Reply::ProjectAdded { id } => id,
        other => panic!("unexpected {other:?}"),
    }
}

fn checkout(snapshot: &Snapshot, project: ProjectId) -> &Workspace {
    snapshot
        .workspaces
        .iter()
        .find(|workspace| workspace.project == project && workspace.kind.is_checkout())
        .expect("the project's checkout")
}

async fn start_shell(connection: &mut Connection, workspace: WorkspaceId) -> SessionId {
    match connection
        .command(Command::StartSession {
            workspace,
            harness: "shell".to_owned(),
            title: None,
            size: TermSize::default(),
            prompt: None,
        })
        .await
        .unwrap()
    {
        Reply::SessionStarted { id } => id,
        other => panic!("unexpected {other:?}"),
    }
}

fn refused(outcome: Result<Reply, ClientError>) -> ErrorCode {
    match outcome {
        Err(ClientError::Daemon(error)) => error.code,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

/// Waits until the workspace's branch, as the daemon tells it, is `wanted`.
async fn wait_for_branch(connection: &mut Connection, workspace: WorkspaceId, wanted: &str) {
    expect_event(connection, |frame| match frame {
        ServerFrame::Delta(Delta::WorkspaceUpserted(found))
            if found.id == workspace && found.kind.branch() == wanted =>
        {
            Some(())
        }
        _ => None,
    })
    .await;
}

#[tokio::test]
async fn a_project_comes_with_its_own_checkout_to_work_in() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    git(&root, &["switch", "--quiet", "-c", "develop"]);
    let daemon = sandbox.start();
    let mut connection = daemon.connect().await;
    let project = add_project(&mut connection, root.clone()).await;

    let snapshot = connection.subscribe().await.unwrap();
    let workspace = checkout(&snapshot, project).clone();
    assert_eq!(workspace.kind.host_path(), Some(root.as_path()));
    assert_eq!(workspace.kind.branch(), "develop");

    // A session there runs in the repository itself.
    let session = start_shell(&mut connection, workspace.id).await;
    connection.attach(session, None).await.unwrap();
    connection
        .input(session, b"pwd; echo here-$((3*4))\n")
        .await
        .unwrap();
    let (text, _) = read_terminal(&mut connection, session, "here-12").await;
    assert!(text.contains(root.to_str().unwrap()), "{text}");

    // Muster never removes it.
    assert_eq!(
        refused(
            connection
                .command(Command::RemoveWorktree {
                    workspace: workspace.id,
                    force: true,
                })
                .await
        ),
        ErrorCode::BadRequest
    );
    assert!(root.join(".git").exists());
}

#[tokio::test]
async fn branches_follow_switches_in_the_checkout_and_in_worktrees() {
    let sandbox = Sandbox::new().with_env("MUSTER_BRANCH_MS", "50");
    let root = sandbox.repository("app");
    let daemon = sandbox.start();
    let mut connection = daemon.connect().await;
    let project = add_project(&mut connection, root.clone()).await;
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
    let snapshot = connection.subscribe().await.unwrap();
    let own = checkout(&snapshot, project).id;
    let worktree_path = snapshot
        .workspaces
        .iter()
        .find(|workspace| workspace.id == worktree)
        .unwrap()
        .kind
        .host_path()
        .unwrap()
        .to_owned();

    git(&root, &["switch", "--quiet", "-c", "elsewhere"]);
    wait_for_branch(&mut connection, own, "elsewhere").await;
    git(&worktree_path, &["switch", "--quiet", "-c", "renamed-work"]);
    wait_for_branch(&mut connection, worktree, "renamed-work").await;
    git(&worktree_path, &["switch", "--quiet", "--detach"]);
    expect_event(&mut connection, |frame| match frame {
        ServerFrame::Delta(Delta::WorkspaceUpserted(found))
            if found.id == worktree && found.kind.branch().starts_with("detached at ") =>
        {
            Some(())
        }
        _ => None,
    })
    .await;

    // What was seen is stored: a new daemon starts from it.
    drop(connection);
    assert!(daemon.stop().success());
    let daemon = sandbox.start();
    let snapshot = daemon.connect().await.subscribe().await.unwrap();
    assert_eq!(checkout(&snapshot, project).kind.branch(), "elsewhere");
}

#[tokio::test]
async fn projects_added_before_checkouts_get_theirs_at_start() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    let paths = sandbox.paths();
    std::fs::create_dir_all(&paths.data_dir).unwrap();
    let project = Project {
        id: ProjectId::new(),
        name: "app".to_owned(),
        root: root.clone(),
        created_at: Timestamp::from_millis(1),
    };
    muster_daemon::store::Store::open(&paths.database())
        .unwrap()
        .insert_project(&project)
        .unwrap();

    let daemon = sandbox.start();
    let snapshot = daemon.connect().await.subscribe().await.unwrap();
    let workspace = checkout(&snapshot, project.id);
    assert_eq!(workspace.kind.host_path(), Some(root.as_path()));
    assert_eq!(workspace.kind.branch(), "main");

    // Only once: a restart does not add another.
    drop(snapshot);
    assert!(daemon.stop().success());
    let daemon = sandbox.start();
    let snapshot = daemon.connect().await.subscribe().await.unwrap();
    assert_eq!(snapshot.workspaces.len(), 1);
}

#[tokio::test]
async fn removing_a_project_forgets_it_and_leaves_the_repository_alone() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    let daemon = sandbox.start();
    let mut connection = daemon.connect().await;
    let project = add_project(&mut connection, root.clone()).await;
    let snapshot = connection.subscribe().await.unwrap();
    let own = checkout(&snapshot, project).id;
    let remove = Command::RemoveProject { project };

    // Not while it has worktrees.
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
    assert_eq!(
        refused(connection.command(remove.clone()).await),
        ErrorCode::Conflict
    );
    connection
        .command(Command::RemoveWorktree {
            workspace: worktree,
            force: false,
        })
        .await
        .unwrap();

    // Not while a session runs in its checkout.
    let session = start_shell(&mut connection, own).await;
    assert_eq!(
        refused(connection.command(remove.clone()).await),
        ErrorCode::Conflict
    );
    connection
        .command(Command::StopSession { session })
        .await
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        match connection.command(remove.clone()).await {
            Ok(reply) => {
                assert_eq!(reply, Reply::Done);
                break;
            }
            Err(ClientError::Daemon(error)) if error.code == ErrorCode::Conflict => {
                assert!(std::time::Instant::now() < deadline, "{error}");
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            Err(other) => panic!("{other}"),
        }
    }

    let snapshot = connection.subscribe().await.unwrap();
    assert!(snapshot.projects.is_empty());
    assert!(snapshot.workspaces.is_empty());
    assert!(snapshot.sessions.is_empty());
    assert!(
        root.join("README.md").exists(),
        "the repository is untouched"
    );
    assert_eq!(git(&root, &["branch", "--list", "work"]), "work");

    // It can be added again, and is still gone after a restart if not.
    let again = add_project(&mut connection, root.clone()).await;
    assert_ne!(again, project);
    connection
        .command(Command::RemoveProject { project: again })
        .await
        .unwrap();
    drop(connection);
    assert!(daemon.stop().success());
    let daemon = sandbox.start();
    let snapshot = daemon.connect().await.subscribe().await.unwrap();
    assert!(snapshot.projects.is_empty());
}

#[test]
fn the_command_line_works_in_a_projects_checkout_and_removes_projects() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    let _daemon = sandbox.start();
    let ok = |args: &[&str]| {
        let output = sandbox.run(args);
        assert!(
            output.status.success(),
            "muster {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    };
    ok(&["project", "add", root.to_str().unwrap()]);
    let status = ok(&["status"]);
    assert!(status.contains("1 project, 0 worktrees"), "{status}");
    assert!(status.contains("(the project's checkout)"), "{status}");

    // The project's name names its checkout.
    let started = ok(&["session", "start", "app"]);
    let session = started
        .trim()
        .trim_start_matches("Started ")
        .trim_end_matches('.');
    ok(&["session", "stop", session]);

    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let output = sandbox.run(&["project", "remove", "app"]);
        if output.status.success() {
            assert!(String::from_utf8_lossy(&output.stdout).contains("untouched"));
            break;
        }
        assert!(std::time::Instant::now() < deadline, "{output:?}");
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(ok(&["status"]).contains("0 projects"));
}

#[test]
fn project_settings_are_shown_with_overrides_and_their_mistakes_explained() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    let _daemon = sandbox.start();
    let run = |args: &[&str]| sandbox.run(args);
    assert!(
        run(&["project", "add", root.to_str().unwrap()])
            .status
            .success()
    );

    let defaults = run(&["project", "config", "app"]);
    let text = String::from_utf8_lossy(&defaults.stdout);
    assert!(defaults.status.success(), "{defaults:?}");
    assert!(text.contains("Neither exists yet"), "{text}");
    assert!(
        text.contains("Runs cannot start yet: runs need checks"),
        "{text}"
    );

    std::fs::write(
        root.join("muster.toml"),
        "[checks]\ncommands = [\"cargo test\"]\n[limits]\nattempts = 2\n",
    )
    .unwrap();
    let status = String::from_utf8_lossy(&run(&["status"]).stdout).into_owned();
    let id = status
        .split("(prj_")
        .nth(1)
        .and_then(|rest| rest.split(')').next())
        .map(|ulid| format!("prj_{ulid}"))
        .unwrap();
    let overrides = sandbox
        .paths()
        .config_dir
        .join("projects")
        .join(format!("{id}.toml"));
    std::fs::create_dir_all(overrides.parent().unwrap()).unwrap();
    std::fs::write(&overrides, "[project]\nmemory = \"8g\"\n").unwrap();
    let shown = run(&["project", "config", "app"]);
    let text = String::from_utf8_lossy(&shown.stdout);
    assert!(text.contains("checks        cargo test"), "{text}");
    assert!(text.contains("2 attempts"), "{text}");
    assert!(text.contains("8.0 GB"), "{text}");
    assert!(text.contains("Runs can use these settings."), "{text}");

    std::fs::write(&overrides, "[project]\nmemroy = \"8g\"\n").unwrap();
    let wrong = run(&["project", "config", "app"]);
    assert!(!wrong.status.success());
    let error = String::from_utf8_lossy(&wrong.stderr);
    assert!(error.contains("memroy"), "{error}");
}

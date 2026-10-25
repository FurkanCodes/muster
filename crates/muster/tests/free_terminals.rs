//! End-to-end tests of terminals in no project (`ARC-33`, `PRODUCT.md`
//! §2): one starts in the home folder, an agent does not; one whose shell
//! is in a project's folder is filed there with its shell running on; one
//! anywhere else is started again there, saying what ends; and the folder
//! one is in becomes a project with it under it. From the real daemon,
//! with a real `/bin/sh`; Muster never types into it.

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

use muster_client::Connection;
use muster_core::protocol::{Command, ErrorCode, Reply, TermSize};
use muster_core::{ProjectId, SessionId, WorkspaceId};

use support::{Sandbox, wait};

/// A sandbox whose home folder is its own.
fn sandbox() -> (Sandbox, PathBuf) {
    let sandbox = Sandbox::new();
    let home = sandbox.root().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let home = home.canonicalize().unwrap();
    let sandbox = sandbox.with_env("HOME", home.to_str().unwrap());
    (sandbox, home)
}

fn real(path: &Path) -> PathBuf {
    path.canonicalize().unwrap()
}

async fn start_free(connection: &mut Connection, harness: &str) -> Result<SessionId, ErrorCode> {
    match connection
        .command(Command::StartSession {
            workspace: None,
            harness: harness.to_owned(),
            title: None,
            size: TermSize::default(),
            prompt: None,
            started_by: None,
            space: None,
        })
        .await
    {
        Ok(Reply::SessionStarted { id }) => Ok(id),
        Ok(other) => panic!("expected SessionStarted, not {other:?}"),
        Err(muster_client::ClientError::Daemon(error)) => Err(error.code),
        Err(error) => panic!("{error}"),
    }
}

async fn add_project(connection: &mut Connection, root: PathBuf) -> (ProjectId, WorkspaceId) {
    let Reply::ProjectAdded { id } = connection
        .command(Command::AddProject {
            root,
            name: None,
            space: None,
        })
        .await
        .unwrap()
    else {
        panic!("expected ProjectAdded");
    };
    let snapshot = connection.subscribe().await.unwrap();
    let checkout = snapshot
        .workspaces
        .iter()
        .find(|workspace| workspace.project == id)
        .unwrap()
        .id;
    (id, checkout)
}

/// Where `session`'s shell is, once it is `wanted`.
async fn shell_in(connection: &mut Connection, session: SessionId, wanted: &Path) -> Reply {
    wait::found_async("the shell in the folder", async || {
        let reply = connection
            .command(Command::ReadPlace { session })
            .await
            .unwrap();
        match &reply {
            Reply::Place {
                folder: Some(folder),
                ..
            } if real(folder) == real(wanted) => Some(reply),
            _ => None,
        }
    })
    .await
}

#[tokio::test]
async fn a_free_terminal_starts_in_the_home_folder_and_an_agent_never_does_arc_33() {
    let (sandbox, home) = sandbox();
    let daemon = sandbox.start();
    let mut connection = daemon.connect().await;
    let shell = start_free(&mut connection, "shell").await.unwrap();
    let Reply::Place {
        folder, workspace, ..
    } = shell_in(&mut connection, shell, &home).await
    else {
        unreachable!()
    };
    assert_eq!(folder.map(|folder| real(&folder)), Some(home));
    assert_eq!(workspace, None);
    let snapshot = connection.subscribe().await.unwrap();
    let stored = snapshot.sessions.iter().find(|s| s.id == shell).unwrap();
    assert!(stored.is_free());

    assert_eq!(
        start_free(&mut connection, "claude").await,
        Err(ErrorCode::BadRequest)
    );
}

#[tokio::test]
async fn a_free_terminal_is_filed_where_its_shell_is_with_its_shell_running_on_arc_33() {
    let (sandbox, _) = sandbox();
    let root = sandbox.repository("app");
    let other = sandbox.repository("lib");
    let daemon = sandbox.start();
    let mut connection = daemon.connect().await;
    let (_, checkout) = add_project(&mut connection, root.clone()).await;
    let (_, elsewhere) = add_project(&mut connection, other).await;
    let shell = start_free(&mut connection, "shell").await.unwrap();

    // Not there yet: refused, nothing moved.
    let refused = connection
        .command(Command::FileSession {
            session: shell,
            workspace: checkout,
        })
        .await
        .unwrap_err();
    assert!(
        refused.to_string().contains("not in that folder"),
        "{refused}"
    );

    let inside = root.join("src");
    std::fs::create_dir_all(&inside).unwrap();
    let typed = format!("cd {}\n", inside.display());
    connection.input(shell, typed.as_bytes()).await.unwrap();
    let Reply::Place { workspace, .. } = shell_in(&mut connection, shell, &inside).await else {
        unreachable!()
    };
    assert_eq!(workspace, Some(checkout));
    // Filed under another project's folder: refused.
    assert!(
        connection
            .command(Command::FileSession {
                session: shell,
                workspace: elsewhere,
            })
            .await
            .is_err()
    );
    connection
        .command(Command::FileSession {
            session: shell,
            workspace: checkout,
        })
        .await
        .unwrap();
    let snapshot = connection.subscribe().await.unwrap();
    let stored = snapshot.sessions.iter().find(|s| s.id == shell).unwrap();
    assert_eq!(stored.workspace, Some(checkout));
    assert!(stored.ended_at.is_none(), "the same shell runs on");
    // The same shell: still where the person took it.
    shell_in(&mut connection, shell, &inside).await;
}

#[tokio::test]
async fn the_folder_a_free_terminal_is_in_becomes_a_project_with_it_under_it() {
    let (sandbox, home) = sandbox();
    let tool = sandbox.root().join("tool");
    std::fs::create_dir_all(&tool).unwrap();
    let daemon = sandbox.start();
    let mut connection = daemon.connect().await;
    let shell = start_free(&mut connection, "shell").await.unwrap();
    shell_in(&mut connection, shell, &home).await;
    // The home folder is no project.
    let refused = connection
        .command(Command::AddProjectHere {
            session: shell,
            space: None,
        })
        .await
        .unwrap_err();
    assert!(refused.to_string().contains("home folder"), "{refused}");

    let typed = format!("cd {}\n", tool.display());
    connection.input(shell, typed.as_bytes()).await.unwrap();
    shell_in(&mut connection, shell, &tool).await;
    let Reply::ProjectAdded { id } = connection
        .command(Command::AddProjectHere {
            session: shell,
            space: None,
        })
        .await
        .unwrap()
    else {
        panic!("expected ProjectAdded");
    };
    let snapshot = connection.subscribe().await.unwrap();
    let project = snapshot.projects.iter().find(|p| p.id == id).unwrap();
    assert_eq!(real(&project.root), real(&tool));
    let stored = snapshot.sessions.iter().find(|s| s.id == shell).unwrap();
    let under = snapshot
        .workspaces
        .iter()
        .find(|workspace| stored.is_in(workspace.id))
        .unwrap();
    assert_eq!(under.project, id);
}

#[tokio::test]
async fn a_terminal_started_again_elsewhere_takes_the_old_one_s_place_and_name() {
    let (sandbox, _) = sandbox();
    let root = sandbox.repository("app");
    let daemon = sandbox.start();
    let mut connection = daemon.connect().await;
    let (_, checkout) = add_project(&mut connection, root).await;
    let shell = start_free(&mut connection, "shell").await.unwrap();
    connection
        .command(Command::RenameSession {
            session: shell,
            title: "Dev server".to_owned(),
        })
        .await
        .unwrap();
    let Reply::SessionStarted { id: new } = connection
        .command(Command::RestartIn {
            session: shell,
            workspace: checkout,
        })
        .await
        .unwrap()
    else {
        panic!("expected SessionStarted");
    };
    let snapshot = wait::found_async("the old terminal gone", async || {
        let snapshot = connection.subscribe().await.unwrap();
        snapshot
            .sessions
            .iter()
            .all(|session| session.id != shell)
            .then_some(snapshot)
    })
    .await;
    let started = snapshot.sessions.iter().find(|s| s.id == new).unwrap();
    assert_eq!(started.workspace, Some(checkout));
    assert_eq!(started.title, "Dev server");
}

//! End-to-end tests of shells beside the agents (`PRODUCT.md` §6): a
//! shell is named by the program in its foreground, as it changes, until
//! the person names it; and it ends with its worktree, which an agent at
//! work there still keeps. From the real daemon, with a real `/bin/sh`.

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

use std::time::{Duration, Instant};

use muster_client::Connection;
use muster_core::protocol::{Command, Reply, TermSize};
use muster_core::{SessionId, WorkspaceId};

use support::Sandbox;

async fn start(connection: &mut Connection, workspace: WorkspaceId, harness: &str) -> SessionId {
    let Reply::SessionStarted { id } = connection
        .command(Command::StartSession {
            workspace,
            harness: harness.to_owned(),
            title: None,
            size: TermSize::default(),
            prompt: None,
        })
        .await
        .unwrap()
    else {
        panic!("expected SessionStarted");
    };
    id
}

/// `session`'s name, as the state says now.
async fn name(connection: &mut Connection, session: SessionId) -> String {
    let snapshot = connection.subscribe().await.unwrap();
    snapshot
        .sessions
        .iter()
        .find(|stored| stored.id == session)
        .map(|stored| stored.title.clone())
        .unwrap_or_default()
}

/// Waits until `session` is named as `wanted` says.
async fn named(
    connection: &mut Connection,
    session: SessionId,
    wanted: impl Fn(&str) -> bool,
) -> String {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let now = name(connection, session).await;
        if wanted(&now) {
            return now;
        }
        assert!(Instant::now() < deadline, "named {now:?}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn a_shell_is_named_by_what_runs_in_it_until_the_person_names_it() {
    let sandbox = Sandbox::new().with_env("MUSTER_SAMPLE_MS", "100");
    let root = sandbox.repository("app");
    let daemon = sandbox.start();
    let mut connection = daemon.connect().await;
    // Shells are named as measured, which is often only while looked at.
    connection.looking(true).await.unwrap();
    connection
        .command(Command::AddProject { root, name: None })
        .await
        .unwrap();
    let checkout = connection.subscribe().await.unwrap().workspaces[0].id;
    let shell = start(&mut connection, checkout, "shell").await;
    // At its prompt: the shell itself, by its program's name.
    let itself = named(&mut connection, shell, |name| name != "Shell").await;
    connection.input(shell, b"sleep 3\n").await.unwrap();
    named(&mut connection, shell, |name| name == "sleep").await;
    named(&mut connection, shell, |name| name == itself).await;

    let Reply::Done = connection
        .command(Command::RenameSession {
            session: shell,
            title: "Dev server".to_owned(),
        })
        .await
        .unwrap()
    else {
        panic!("expected Done");
    };
    connection.input(shell, b"sleep 2\n").await.unwrap();
    tokio::time::sleep(Duration::from_millis(800)).await;
    assert_eq!(name(&mut connection, shell).await, "Dev server");
}

#[tokio::test]
async fn a_worktree_s_shell_ends_with_it_but_an_agent_at_work_keeps_it() {
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
    let Reply::WorktreeCreated { id: worktree, .. } = connection
        .command(Command::CreateWorktree {
            project,
            branch: None,
            on: None,
        })
        .await
        .unwrap()
    else {
        panic!("expected WorktreeCreated");
    };
    let shell = start(&mut connection, worktree, "shell").await;
    let agent = start(&mut connection, worktree, "claude").await;
    let remove = Command::RemoveWorktree {
        workspace: worktree,
        force: false,
    };
    let refused = connection.command(remove.clone()).await.unwrap_err();
    assert!(
        refused.to_string().contains("1 running session"),
        "{refused}"
    );

    let Reply::Done = connection
        .command(Command::StopSession { session: agent })
        .await
        .unwrap()
    else {
        panic!("expected Done");
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match connection.command(remove.clone()).await {
            Ok(_) => break,
            Err(error) => {
                assert!(Instant::now() < deadline, "{error}");
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
    let snapshot = connection.subscribe().await.unwrap();
    assert!(snapshot.sessions.iter().all(|session| session.id != shell));
    assert!(snapshot.workspaces.iter().all(|found| found.id != worktree));
}

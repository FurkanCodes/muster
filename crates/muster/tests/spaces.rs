//! End-to-end tests of spaces (`PRODUCT.md` §2): made, renamed and
//! removed, with projects and free terminals in them; what is in one
//! leaves it as it goes; two clients see the same; the CLI lists them.
//! From the real daemon.

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

use muster_client::Connection;
use muster_core::protocol::{Command, Reply, TermSize};
use muster_core::spaces::Member;
use muster_core::{Space, SpaceId};

use support::{Sandbox, wait};

async fn make(connection: &mut Connection, name: &str) -> SpaceId {
    let Reply::SpaceCreated { id } = connection
        .command(Command::CreateSpace {
            name: name.to_owned(),
        })
        .await
        .unwrap()
    else {
        panic!("expected SpaceCreated");
    };
    id
}

async fn spaces(connection: &mut Connection) -> Vec<Space> {
    connection.subscribe().await.unwrap().spaces
}

#[tokio::test]
async fn spaces_hold_projects_and_free_terminals_and_let_them_go_as_they_go() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    let daemon = sandbox.start();
    let mut connection = daemon.connect().await;
    let mut other = daemon.connect().await;
    let work = make(&mut connection, "Work").await;
    let refused = connection
        .command(Command::CreateSpace {
            name: "work".to_owned(),
        })
        .await
        .unwrap_err();
    assert!(refused.to_string().contains("already"), "{refused}");

    // A project added in the space shown joins it.
    let Reply::ProjectAdded { id: project } = connection
        .command(Command::AddProject {
            root,
            name: None,
            space: Some(work),
        })
        .await
        .unwrap()
    else {
        panic!("expected ProjectAdded");
    };
    let Reply::SessionStarted { id: free } = connection
        .command(Command::StartSession {
            workspace: None,
            harness: "shell".to_owned(),
            title: None,
            size: TermSize::default(),
            prompt: None,
            started_by: None,
            space: Some(work),
        })
        .await
        .unwrap()
    else {
        panic!("expected SessionStarted");
    };
    // The other client sees the same.
    let seen = spaces(&mut other).await;
    assert_eq!(seen.len(), 1);
    assert_eq!(
        seen[0].members,
        [Member::Project(project), Member::Session(free)]
    );

    connection
        .command(Command::RenameSpace {
            space: work,
            name: "Client".to_owned(),
        })
        .await
        .unwrap();
    connection
        .command(Command::RemoveFromSpace {
            space: work,
            member: Member::Project(project),
        })
        .await
        .unwrap();
    assert_eq!(spaces(&mut other).await[0].name, "Client");

    // A terminal stopped and taken off the list leaves its space.
    connection
        .command(Command::StopSession { session: free })
        .await
        .unwrap();
    wait::until_async("the terminal taken off the list", async || {
        connection
            .command(Command::RemoveSession { session: free })
            .await
            .is_ok()
    })
    .await;
    assert!(spaces(&mut connection).await[0].members.is_empty());

    // What was in a space stays when it goes.
    connection
        .command(Command::AddToSpace {
            space: work,
            member: Member::Project(project),
        })
        .await
        .unwrap();
    connection
        .command(Command::DeleteSpace { space: work })
        .await
        .unwrap();
    let snapshot = connection.subscribe().await.unwrap();
    assert!(snapshot.spaces.is_empty());
    assert_eq!(snapshot.projects.len(), 1);
}

#[tokio::test]
async fn a_space_and_its_members_outlast_the_daemon() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    let space = {
        let daemon = sandbox.start();
        let mut connection = daemon.connect().await;
        let space = make(&mut connection, "Work").await;
        let Reply::ProjectAdded { id } = connection
            .command(Command::AddProject {
                root,
                name: None,
                space: Some(space),
            })
            .await
            .unwrap()
        else {
            panic!("expected ProjectAdded");
        };
        (space, id)
    };
    let daemon = sandbox.start();
    let mut connection = daemon.connect().await;
    let kept = spaces(&mut connection).await;
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].id, space.0);
    assert_eq!(kept[0].members, [Member::Project(space.1)]);
}

#[tokio::test]
async fn the_command_line_makes_lists_renames_and_removes_spaces() {
    let sandbox = Sandbox::new();
    let _daemon = sandbox.start();
    let made = sandbox.run(&["space", "add", "Client", "work"]);
    assert!(made.status.success(), "{made:?}");
    let listed = String::from_utf8(sandbox.run(&["list", "spaces"]).stdout).unwrap();
    assert!(listed.contains("Client work  (empty)"), "{listed}");
    let renamed = sandbox.run(&["space", "rename", "client work", "Home"]);
    assert!(renamed.status.success(), "{renamed:?}");
    let free = sandbox.run(&["term", "new", "--free", "--space", "Home", "--bg"]);
    assert!(free.status.success(), "{free:?}");
    let listed = String::from_utf8(
        sandbox
            .run(&["list", "terminals", "--space", "Home"])
            .stdout,
    )
    .unwrap();
    assert!(listed.contains("Terminals"), "{listed}");
    let status: serde_json::Value =
        serde_json::from_slice(&sandbox.run(&["status", "--json"]).stdout).unwrap();
    assert_eq!(
        status["terminals"].as_array().map(Vec::len),
        Some(1),
        "{status}"
    );
    assert_eq!(status["spaces"][0]["name"], "Home", "{status}");
    assert_eq!(
        status["spaces"][0]["members"].as_array().map(Vec::len),
        Some(1)
    );
    let removed = sandbox.run(&["space", "rm", "Home"]);
    assert!(removed.status.success(), "{removed:?}");
    let listed = String::from_utf8(sandbox.run(&["list", "spaces"]).stdout).unwrap();
    assert!(listed.contains("No spaces"), "{listed}");
}

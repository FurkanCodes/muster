//! End-to-end test of where the work in a project's workspaces meets
//! (`PRODUCT.md` §6): an agent's turn in a worktree ending with a change to
//! the line the person changed in the checkout shows both that merging
//! them would conflict, from the real daemon on a real repository, which
//! changes nothing of either (`PRD-14`).

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

use muster_core::WorkspaceKind;
use muster_core::protocol::{Command, Delta, Reply, ServerFrame, TermSize};
use serde_json::json;

use support::{Sandbox, expect_event};

#[tokio::test]
async fn an_agent_changing_the_line_the_person_changed_is_shown_to_conflict() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    let daemon = sandbox.start();
    let mut connection = daemon.connect().await;
    let Reply::ProjectAdded { id: project } = connection
        .command(Command::AddProject {
            root: root.clone(),
            name: None,
        })
        .await
        .unwrap()
    else {
        panic!("expected ProjectAdded");
    };
    let Reply::WorktreeCreated { id: workspace } = connection
        .command(Command::CreateWorktree {
            project,
            branch: Some("work".to_owned()),
        })
        .await
        .unwrap()
    else {
        panic!("expected WorktreeCreated");
    };
    let snapshot = connection.subscribe().await.unwrap();
    let path = snapshot
        .workspaces
        .iter()
        .find(|found| found.id == workspace)
        .and_then(|found| found.kind.host_path())
        .unwrap()
        .to_owned();
    let checkout = snapshot
        .workspaces
        .iter()
        .find(|found| matches!(found.kind, WorkspaceKind::Checkout { .. }))
        .unwrap()
        .id;

    // The person's own change in the checkout, looked at.
    std::fs::write(root.join("README.md"), "test\nby the person\n").unwrap();
    connection
        .command(Command::WorkspaceChanges {
            workspace: checkout,
        })
        .await
        .unwrap();

    let Reply::SessionStarted { id: session } = connection
        .command(Command::StartSession {
            workspace,
            harness: "claude".to_owned(),
            title: None,
            size: TermSize::default(),
            prompt: None,
        })
        .await
        .unwrap()
    else {
        panic!("expected SessionStarted");
    };
    let launch = sandbox.wait_for_launches(session, 1).remove(0);
    let id = launch.value("--session-id").unwrap().to_owned();
    let fire = |event: &str| {
        launch.fire(
            event,
            &json!({ "session_id": id, "hook_event_name": event, "cwd": "/work" }),
        );
    };
    fire("UserPromptSubmit");
    std::fs::write(path.join("README.md"), "test\nby the agent\n").unwrap();
    fire("Stop");

    let clashes = expect_event(&mut connection, |frame| match frame {
        ServerFrame::Delta(Delta::WorkspaceUpserted(shown))
            if shown.id == workspace && !shown.clashes.is_empty() =>
        {
            Some(shown.clashes.clone())
        }
        _ => None,
    })
    .await;
    assert_eq!(clashes.len(), 1);
    assert_eq!(clashes[0].with, checkout);
    assert!(clashes[0].would_conflict());
    assert_eq!(clashes[0].shown[0].path, "README.md");

    // Only read: both still have what they wrote.
    assert_eq!(
        std::fs::read_to_string(root.join("README.md")).unwrap(),
        "test\nby the person\n"
    );
    assert_eq!(
        std::fs::read_to_string(path.join("README.md")).unwrap(),
        "test\nby the agent\n"
    );
    // Nothing of the person's code reaches the daemon's log.
    let log = daemon.log();
    assert!(!log.contains("by the"), "{log}");
}

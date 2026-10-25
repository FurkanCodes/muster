//! End-to-end tests for what a workspace's work has changed (`PRODUCT.md`
//! §6): shown on the worktree as soon as an agent's turn there ends, and
//! asked for in full, with a file's diff, from the real daemon on a real
//! repository.

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

use muster_core::changes::{ChangeKind, ChangePart, ChangeSummary};
use muster_core::protocol::{Command, Delta, Reply, ServerFrame, TermSize};
use serde_json::json;

use support::{Sandbox, expect_event};

#[tokio::test]
#[expect(clippy::too_many_lines, reason = "one end-to-end story, step by step")]
async fn an_agent_s_turn_ending_shows_what_it_changed_and_each_file_s_diff() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    let daemon = sandbox.start();
    let mut connection = daemon.connect().await;
    let Reply::ProjectAdded { id: project } = connection
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
    let Reply::WorktreeCreated { id: workspace } = connection
        .command(Command::CreateWorktree {
            project,
            branch: Some("work".to_owned()),
            on: None,
            named_for: None,
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

    let Reply::SessionStarted { id: session } = connection
        .command(Command::StartSession {
            workspace: Some(workspace),
            harness: "claude".to_owned(),
            title: None,
            size: TermSize::default(),
            prompt: None,
            started_by: None,
            space: None,
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

    // A turn: the agent writes a file, and the turn ends.
    fire("UserPromptSubmit");
    std::fs::write(path.join("app.rs"), "fn main() {}\n").unwrap();
    fire("Stop");
    let shown = expect_event(&mut connection, |frame| match frame {
        ServerFrame::Delta(Delta::WorkspaceUpserted(shown))
            if shown.id == workspace && shown.changes.is_some() =>
        {
            shown.changes
        }
        _ => None,
    })
    .await;
    assert_eq!(
        shown,
        ChangeSummary {
            files: 1,
            added: 1,
            removed: 0,
        }
    );

    let Reply::Changes(changes) = connection
        .command(Command::WorkspaceChanges { workspace })
        .await
        .unwrap()
    else {
        panic!("expected the worktree's changes");
    };
    assert_eq!(changes.base.as_deref(), Some("main"));
    assert_eq!(changes.uncommitted.len(), 1);
    assert_eq!(changes.uncommitted[0].path, "app.rs");
    assert_eq!(changes.uncommitted[0].kind, ChangeKind::Untracked);

    let Reply::FileDiff(diff) = connection
        .command(Command::FileDiff {
            workspace,
            part: ChangePart::Uncommitted,
            path: "app.rs".to_owned(),
        })
        .await
        .unwrap()
    else {
        panic!("expected the file's diff");
    };
    assert!(diff.text.contains("+fn main() {}"), "{}", diff.text);

    // Nothing of the person's code reaches the daemon's log.
    let log = daemon.log();
    assert!(!log.contains("fn main"), "{log}");
}

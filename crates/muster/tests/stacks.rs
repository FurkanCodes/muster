//! End-to-end test of stacked work (`PRODUCT.md` §6, `PRD-14`): a worktree
//! made on top of another's branch, measured against it, and moved onto
//! its newest commit at the person's request, by the real daemon on a real
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

use std::path::{Path, PathBuf};

use muster_client::Connection;
use muster_core::protocol::{Command, Delta, Reply, ServerFrame};
use muster_core::{ProjectId, WorkspaceId};

use support::{Sandbox, expect_event, git};

fn commit(dir: &Path, file: &str) {
    std::fs::write(dir.join(file), format!("{file}\n")).unwrap();
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "--quiet", "-m", file]);
}

async fn worktree(
    connection: &mut Connection,
    project: ProjectId,
    branch: &str,
    on: Option<WorkspaceId>,
) -> (WorkspaceId, PathBuf) {
    let Reply::WorktreeCreated { id } = connection
        .command(Command::CreateWorktree {
            project,
            branch: Some(branch.to_owned()),
            on,
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
        .find(|found| found.id == id)
        .and_then(|found| found.kind.host_path())
        .unwrap()
        .to_owned();
    (id, path)
}

#[tokio::test]
async fn a_worktree_on_top_of_another_is_measured_against_it_and_moved_onto_its_newest() {
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
    let (parent, parent_path) = worktree(&mut connection, project, "parent", None).await;
    commit(&parent_path, "parent.txt");
    let first = git(&parent_path, &["rev-parse", "HEAD"]);

    let (child, child_path) = worktree(&mut connection, project, "child", Some(parent)).await;
    assert_eq!(git(&child_path, &["rev-parse", "HEAD"]), first);
    let snapshot = connection.subscribe().await.unwrap();
    let stacked = snapshot
        .workspaces
        .iter()
        .find(|found| found.id == child)
        .and_then(|found| found.stacked_on.clone())
        .unwrap();
    assert_eq!(
        (stacked.branch.as_str(), stacked.at.as_str()),
        ("parent", first.as_str())
    );

    // Its work is its own commits, against the branch it is on top of.
    commit(&child_path, "child.txt");
    let Reply::Changes(changes) = connection
        .command(Command::WorkspaceChanges { workspace: child })
        .await
        .unwrap()
    else {
        panic!("expected Changes");
    };
    assert_eq!(changes.base.as_deref(), Some("parent"));
    let paths: Vec<&str> = changes
        .branch
        .iter()
        .map(|file| file.path.as_str())
        .collect();
    assert_eq!(paths, ["child.txt"]);

    // The parent moves on; the child follows when asked.
    commit(&parent_path, "more.txt");
    let newest = git(&parent_path, &["rev-parse", "HEAD"]);
    let Reply::Moved { onto, commits, .. } = connection
        .command(Command::MoveOnto { workspace: child })
        .await
        .unwrap()
    else {
        panic!("expected Moved");
    };
    assert_eq!((onto.as_str(), commits), ("parent", 1));
    assert_eq!(git(&child_path, &["rev-parse", "HEAD~1"]), newest);
    let now = expect_event(&mut connection, |frame| match frame {
        ServerFrame::Delta(Delta::WorkspaceUpserted(shown)) if shown.id == child => shown
            .stacked_on
            .clone()
            .filter(|stacked| stacked.at == newest),
        _ => None,
    })
    .await;
    assert_eq!(now.branch, "parent");
}

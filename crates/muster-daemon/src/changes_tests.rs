//! What each workspace's work has changed (`PRODUCT.md` §6), asked of the
//! Supervisor on a real repository: the whole for a client, a file's
//! diff, and the summary kept on the workspace.

use std::path::PathBuf;
use std::time::Duration;

use muster_core::changes::{ChangeKind, ChangePart, ChangeSummary};
use muster_core::protocol::{Command, Delta, ErrorCode, Reply};
use muster_core::{WorkspaceId, WorkspaceKind};
use tokio::sync::broadcast;
use tokio::time::timeout;

use crate::pulls_tests::{Setup, setup};
use crate::supervisor::SupervisorHandle;

/// Where `workspace`'s files are.
async fn path_of(supervisor: &SupervisorHandle, workspace: WorkspaceId) -> PathBuf {
    let (snapshot, _) = supervisor.subscribe().await.unwrap();
    snapshot
        .workspaces
        .into_iter()
        .find(|found| found.id == workspace)
        .and_then(|found| found.kind.host_path().map(PathBuf::from))
        .unwrap()
}

/// The next summary shown on `workspace`.
async fn next_summary(
    deltas: &mut broadcast::Receiver<Delta>,
    workspace: WorkspaceId,
) -> Option<ChangeSummary> {
    timeout(Duration::from_secs(10), async {
        loop {
            if let Delta::WorkspaceUpserted(shown) = deltas.recv().await.unwrap()
                && shown.id == workspace
            {
                return shown.changes;
            }
        }
    })
    .await
    .expect("the workspace's changes were never shown")
}

#[tokio::test]
async fn a_client_asks_for_a_worktree_s_changes_and_its_line_follows() {
    // The repository lives as long as the test.
    let Setup {
        _dir,
        supervisor,
        worktree,
        ..
    } = setup().await;
    let path = path_of(&supervisor, worktree).await;
    std::fs::write(path.join("notes.md"), "one\ntwo\n").unwrap();
    let (_, mut deltas) = supervisor.subscribe().await.unwrap();

    let Ok(Reply::Changes(changes)) = supervisor
        .command(Command::WorkspaceChanges {
            workspace: worktree,
        })
        .await
    else {
        panic!("expected the worktree's changes");
    };
    assert_eq!(changes.base.as_deref(), Some("main"));
    assert_eq!(changes.uncommitted.len(), 1);
    assert_eq!(changes.uncommitted[0].kind, ChangeKind::Untracked);
    let summary = Some(ChangeSummary {
        files: 1,
        added: 2,
        removed: 0,
    });
    assert_eq!(next_summary(&mut deltas, worktree).await, summary);

    let Ok(Reply::FileDiff(diff)) = supervisor
        .command(Command::FileDiff {
            workspace: worktree,
            part: ChangePart::Uncommitted,
            path: "notes.md".to_owned(),
        })
        .await
    else {
        panic!("expected the file's diff");
    };
    assert!(diff.text.contains("+two"), "{}", diff.text);
    let refused = supervisor
        .command(Command::FileDiff {
            workspace: worktree,
            part: ChangePart::Uncommitted,
            path: "README.md".to_owned(),
        })
        .await
        .unwrap_err();
    assert_eq!(refused.code, ErrorCode::NotFound);
}

#[tokio::test]
async fn the_timer_looks_again_and_shows_what_changed_since() {
    // The repository lives as long as the test.
    let Setup {
        _dir,
        supervisor,
        worktree,
        ..
    } = setup().await;
    let path = path_of(&supervisor, worktree).await;
    let (_, mut deltas) = supervisor.subscribe().await.unwrap();
    std::fs::write(path.join("README.md"), "test\nmore\n").unwrap();
    assert!(supervisor.check_pull_requests().await);
    assert_eq!(
        next_summary(&mut deltas, worktree).await,
        Some(ChangeSummary {
            files: 1,
            added: 1,
            removed: 0,
        })
    );
    // Put back: nothing changed any more.
    std::fs::write(path.join("README.md"), "test\n").unwrap();
    assert!(supervisor.check_pull_requests().await);
    assert_eq!(next_summary(&mut deltas, worktree).await, None);
}

#[tokio::test]
async fn a_plain_folder_has_no_changes_to_ask_for() {
    let Setup {
        _dir: dir,
        supervisor,
        ..
    } = setup().await;
    let folder = dir.path().join("notes");
    std::fs::create_dir(&folder).unwrap();
    supervisor
        .command(Command::AddProject {
            root: folder.clone(),
            name: None,
        })
        .await
        .unwrap();
    let (snapshot, _) = supervisor.subscribe().await.unwrap();
    let checkout = snapshot
        .workspaces
        .iter()
        .find(|found| {
            matches!(&found.kind, WorkspaceKind::Checkout { path, .. } if path.ends_with("notes"))
        })
        .unwrap()
        .id;
    let refused = supervisor
        .command(Command::WorkspaceChanges {
            workspace: checkout,
        })
        .await
        .unwrap_err();
    assert_eq!(refused.code, ErrorCode::BadRequest);
}

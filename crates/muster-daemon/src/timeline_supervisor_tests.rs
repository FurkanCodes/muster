//! Workspaces' timelines asked of the Supervisor (`PRODUCT.md` §6): what
//! it answers before any turn, and where there is no timeline to keep.
//! Turns recorded and undone through the real daemon are in
//! `crates/muster/tests/timeline.rs`.

use muster_core::protocol::{Command, ErrorCode, Reply};
use muster_core::timeline::Timeline;
use muster_core::{WorkspaceId, WorkspaceKind};

use crate::pulls_tests::{Setup, setup};

#[tokio::test]
async fn a_workspace_with_no_turn_yet_has_an_empty_timeline_and_no_entry() {
    let Setup {
        _dir: dir,
        supervisor,
        worktree,
        ..
    } = setup().await;
    let Ok(Reply::Timeline(timeline)) = supervisor
        .command(Command::Timeline {
            workspace: worktree,
        })
        .await
    else {
        panic!("expected a timeline");
    };
    assert_eq!(timeline, Timeline::default());
    for command in [
        Command::EntryDiff {
            workspace: worktree,
            number: 1,
        },
        Command::UndoPlan {
            workspace: worktree,
            number: 1,
        },
        Command::Undo {
            workspace: worktree,
            number: 1,
            move_branch: false,
        },
    ] {
        let refused = supervisor.command(command).await.unwrap_err();
        assert_eq!(refused.code, ErrorCode::NotFound, "{refused:?}");
    }
    // Asking made no store.
    assert!(
        !dir.path()
            .join("data/timelines")
            .join(worktree.to_string())
            .exists()
    );
    let unknown = supervisor
        .command(Command::Timeline {
            workspace: WorkspaceId::new(),
        })
        .await
        .unwrap_err();
    assert_eq!(unknown.code, ErrorCode::NotFound);
}

#[tokio::test]
async fn a_plain_folder_keeps_no_timeline() {
    let Setup {
        _dir: dir,
        supervisor,
        ..
    } = setup().await;
    let folder = dir.path().join("notes");
    std::fs::create_dir(&folder).unwrap();
    supervisor
        .command(Command::AddProject {
            root: folder,
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
        .command(Command::Timeline {
            workspace: checkout,
        })
        .await
        .unwrap_err();
    assert_eq!(refused.code, ErrorCode::BadRequest);
}

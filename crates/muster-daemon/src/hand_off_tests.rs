//! Handing a person's work to the autopilot (`PRD-08`): a run that takes
//! over a worktree's branch, as committed, and starts at the checks.

use std::path::PathBuf;

use muster_core::protocol::{Command, ErrorCode, Reply};
use muster_core::{RunStage, WorkspaceId};
use muster_state::run::{RunEffect, RunEvent};

use super::*;
use crate::git::tests::{git, repository};

/// A supervisor on a real repository whose `muster.toml` is committed.
async fn real_setup() -> Setup {
    let dir = tempfile::tempdir().unwrap();
    let root = repository(dir.path(), "app");
    std::fs::write(root.join("muster.toml"), CHECKS).unwrap();
    git(&root, &["add", "muster.toml"]);
    git(&root, &["commit", "-q", "-m", "Runs"]);
    let database = dir.path().join("muster.db");
    let work = Arc::new(FakeWork::default());
    let supervisor = spawn(&database, work.clone(), dir.path()).await;
    let Reply::ProjectAdded { id: project } = supervisor
        .command(Command::AddProject {
            root: root.clone(),
            name: None,
        })
        .await
        .unwrap()
    else {
        panic!("expected ProjectAdded");
    };
    Setup {
        _dir: dir,
        root,
        database,
        work,
        supervisor,
        project,
    }
}

async fn worktree(setup: &Setup, branch: &str) -> (WorkspaceId, PathBuf) {
    let Reply::WorktreeCreated { id } = setup
        .supervisor
        .command(Command::CreateWorktree {
            project: setup.project,
            branch: Some(branch.to_owned()),
        })
        .await
        .unwrap()
    else {
        panic!("expected WorktreeCreated");
    };
    let (snapshot, _) = setup.supervisor.subscribe().await.unwrap();
    let path = snapshot
        .workspaces
        .iter()
        .find(|workspace| workspace.id == id)
        .and_then(|workspace| workspace.kind.host_path())
        .unwrap()
        .to_owned();
    (id, path)
}

fn hand_off(setup: &Setup, from: WorkspaceId) -> Command {
    Command::StartRun {
        project: setup.project,
        number: 42,
        from: Some(from),
    }
}

#[tokio::test]
async fn a_worktree_s_committed_work_goes_to_the_checks_first() {
    let setup = real_setup().await;
    let (_, mut deltas) = setup.supervisor.subscribe().await.unwrap();
    let (feature, path) = worktree(&setup, "feature").await;

    // Uncommitted work would be left out: refused, saying so.
    std::fs::write(path.join("fix.txt"), "fixed\n").unwrap();
    let dirty = setup
        .supervisor
        .command(hand_off(&setup, feature))
        .await
        .unwrap_err();
    assert_eq!(dirty.code, ErrorCode::Conflict);
    assert!(
        dirty.message.contains("1 uncommitted change(s)"),
        "{}",
        dirty.message
    );

    git(&path, &["add", "fix.txt"]);
    git(&path, &["commit", "-q", "-m", "Fix by hand"]);
    let Reply::RunStarted { id: run } = setup
        .supervisor
        .command(hand_off(&setup, feature))
        .await
        .unwrap()
    else {
        panic!("expected RunStarted");
    };
    let (context, effect) = setup.work.asked(1).await;
    assert!(matches!(effect, RunEffect::StartContainer { .. }));
    assert_eq!(context.branch, "feature", "the person's branch, as it is");
    assert!(context.from_branch);
    assert_eq!(context.ticket.number, 42);

    // Ready, its container goes to the checks, not to an agent.
    setup.work.report(run, Report::Container("c-1".to_owned()));
    setup.work.report(
        run,
        Report::Event(RunEvent::ContainerReady {
            generation: generation(&effect),
        }),
    );
    wait_for_stage(&mut deltas, run, RunStage::Checking).await;
    let (_, effect) = setup.work.asked(2).await;
    assert!(matches!(effect, RunEffect::RunChecks { .. }), "{effect:?}");

    // One run per branch.
    let twice = setup
        .supervisor
        .command(Command::StartRun {
            project: setup.project,
            number: 7,
            from: Some(feature),
        })
        .await
        .unwrap_err();
    assert_eq!(twice.code, ErrorCode::Conflict);
    assert!(
        twice.message.contains("feature is already a run's"),
        "{}",
        twice.message
    );

    // A restarted daemon still knows the run took over a branch.
    let Setup {
        _dir: dir,
        database,
        supervisor,
        ..
    } = setup;
    drop(supervisor);
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let work = Arc::new(FakeWork::default());
    let _supervisor = spawn(&database, work.clone(), dir.path()).await;
    let (context, effect) = work.asked(1).await;
    assert!(matches!(effect, RunEffect::RunChecks { .. }));
    assert!(context.from_branch);
}

#[tokio::test]
async fn work_that_cannot_go_to_a_run_is_refused_plainly() {
    let setup = real_setup().await;
    let (snapshot, _) = setup.supervisor.subscribe().await.unwrap();
    let checkout = snapshot
        .workspaces
        .iter()
        .find(|workspace| workspace.kind.is_checkout())
        .unwrap()
        .id;

    // The checkout is on the base branch the config names.
    std::fs::write(
        setup.root.join("muster.toml"),
        format!("{CHECKS}[project]\nbase_branch = \"main\"\n"),
    )
    .unwrap();
    git(&setup.root, &["commit", "-q", "-am", "Base"]);
    let base = setup
        .supervisor
        .command(hand_off(&setup, checkout))
        .await
        .unwrap_err();
    assert_eq!(base.code, ErrorCode::BadRequest);
    assert!(
        base.message.contains("main is the branch runs start from"),
        "{}",
        base.message
    );

    // A workspace that is not there, or not this project's.
    let missing = setup
        .supervisor
        .command(hand_off(&setup, WorkspaceId::new()))
        .await
        .unwrap_err();
    assert_eq!(missing.code, ErrorCode::NotFound);
    let other_root = repository(setup._dir.path(), "other");
    let Reply::ProjectAdded { id: other } = setup
        .supervisor
        .command(Command::AddProject {
            root: other_root,
            name: None,
        })
        .await
        .unwrap()
    else {
        panic!("expected ProjectAdded");
    };
    let (snapshot, _) = setup.supervisor.subscribe().await.unwrap();
    let foreign = snapshot
        .workspaces
        .iter()
        .find(|workspace| workspace.project == other)
        .unwrap()
        .id;
    let elsewhere = setup
        .supervisor
        .command(hand_off(&setup, foreign))
        .await
        .unwrap_err();
    assert!(
        elsewhere.message.contains("another project"),
        "{}",
        elsewhere.message
    );
    assert_eq!(setup.work.count(), 0, "nothing started");
}

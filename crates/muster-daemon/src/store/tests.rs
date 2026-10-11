use std::path::PathBuf;

use muster_core::{
    Project, ProjectId, Session, SessionId, SessionStatus, Timestamp, WaitReason, Workspace,
    WorkspaceId, WorkspaceKind,
};
use rusqlite::Connection;

use super::migrations::{MIGRATIONS, latest_version};
use super::{ResourceSample, StatusChange, Store, StoreError, StoreHandle};

fn at(millis: i64) -> Timestamp {
    Timestamp::from_millis(millis)
}

fn project(root: &str) -> Project {
    Project {
        id: ProjectId::new(),
        name: "app".to_owned(),
        root: PathBuf::from(root),
        created_at: at(1),
    }
}

fn worktree(project: ProjectId) -> Workspace {
    Workspace {
        id: WorkspaceId::new(),
        project,
        kind: WorkspaceKind::Worktree {
            path: PathBuf::from("/data/worktrees/app/fix-login"),
            branch: "muster/fix-login".to_owned(),
        },
        created_at: at(2),
    }
}

fn session(workspace: WorkspaceId) -> Session {
    Session {
        id: SessionId::new(),
        workspace,
        harness: "claude".to_owned(),
        title: "Fix the login redirect".to_owned(),
        status: SessionStatus::Fresh,
        unseen: false,
        resume_id: None,
        created_at: at(3),
        ended_at: None,
    }
}

/// A store holding one project, worktree and session.
fn populated() -> (Store, Project, Workspace, Session) {
    let mut store = Store::open_in_memory().unwrap();
    let project = project("/src/app");
    let workspace = worktree(project.id);
    let session = session(workspace.id);
    store.insert_project(&project).unwrap();
    store.insert_workspace(&workspace).unwrap();
    store.insert_session(&session).unwrap();
    (store, project, workspace, session)
}

// Migrations

#[test]
fn a_new_database_is_at_the_latest_schema() {
    let store = Store::open_in_memory().unwrap();
    assert_eq!(store.schema_version().unwrap(), latest_version());
}

#[test]
fn every_older_schema_migrates_to_the_latest() {
    for applied in 0..MIGRATIONS.len() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("muster.db");
        {
            let conn = Connection::open(&path).unwrap();
            for migration in &MIGRATIONS[..applied] {
                conn.execute_batch(migration).unwrap();
            }
            conn.pragma_update(None, "user_version", applied as i64)
                .unwrap();
        }
        let store = Store::open(&path).unwrap();
        assert_eq!(store.schema_version().unwrap(), latest_version());
        let problems: i64 = store
            .conn
            .query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(problems, 0, "after migrating from version {applied}");
    }
}

#[test]
fn a_database_from_a_newer_muster_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("muster.db");
    Connection::open(&path)
        .unwrap()
        .pragma_update(None, "user_version", latest_version() + 1)
        .unwrap();
    let error = Store::open(&path).unwrap_err();
    assert!(matches!(error, StoreError::NewerSchema { .. }));
}

#[test]
fn data_survives_reopening() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("muster.db");
    let project = project("/src/app");
    Store::open(&path)
        .unwrap()
        .insert_project(&project)
        .unwrap();

    let reopened = Store::open(&path).unwrap();
    assert_eq!(reopened.snapshot().unwrap().projects, [project]);
}

// Entities

#[test]
fn the_snapshot_returns_what_was_stored() {
    let (store, project, workspace, session) = populated();
    let snapshot = store.snapshot().unwrap();
    assert_eq!(snapshot.projects, [project]);
    assert_eq!(snapshot.workspaces, [workspace]);
    assert_eq!(snapshot.sessions, [session]);
}

#[test]
fn a_second_project_at_the_same_root_conflicts() {
    let (mut store, _, _, _) = populated();
    let error = store.insert_project(&project("/src/app")).unwrap_err();
    assert!(matches!(error, StoreError::Conflict(_)));
    assert!(
        store
            .project_by_root("/src/app".as_ref())
            .unwrap()
            .is_some()
    );
    assert!(
        store
            .project_by_root("/src/other".as_ref())
            .unwrap()
            .is_none()
    );
}

#[test]
fn a_workspace_needs_an_existing_project() {
    let mut store = Store::open_in_memory().unwrap();
    let error = store
        .insert_workspace(&worktree(ProjectId::new()))
        .unwrap_err();
    assert!(matches!(error, StoreError::Sqlite(_)));
}

#[test]
fn a_removed_workspace_leaves_the_snapshot_with_its_sessions() {
    let (mut store, _, workspace, _) = populated();
    store.remove_workspace(workspace.id, at(50)).unwrap();
    let snapshot = store.snapshot().unwrap();
    assert!(snapshot.workspaces.is_empty());
    assert!(snapshot.sessions.is_empty());
    assert!(matches!(
        store.remove_workspace(workspace.id, at(60)),
        Err(StoreError::NotFound(_))
    ));
}

#[test]
fn a_status_change_updates_the_session_and_its_history_together() {
    let (mut store, _, _, session) = populated();
    let waiting = SessionStatus::NeedsYou {
        reason: WaitReason::Question,
    };
    store
        .record_status(session.id, SessionStatus::Running, false, at(10))
        .unwrap();
    store
        .record_status(session.id, waiting, true, at(20))
        .unwrap();

    let stored = &store.snapshot().unwrap().sessions[0];
    assert_eq!((stored.status, stored.unseen), (waiting, true));
    assert_eq!(
        store.status_history(session.id).unwrap(),
        [
            StatusChange {
                status: SessionStatus::Running,
                unseen: false,
                at: at(10),
            },
            StatusChange {
                status: waiting,
                unseen: true,
                at: at(20),
            },
        ]
    );
}

#[test]
fn a_status_change_for_an_unknown_session_writes_nothing() {
    let (mut store, _, _, _) = populated();
    let unknown = SessionId::new();
    let error = store
        .record_status(unknown, SessionStatus::Running, false, at(10))
        .unwrap_err();
    assert!(matches!(error, StoreError::NotFound(_)));
    assert!(store.status_history(unknown).unwrap().is_empty());
}

#[test]
fn resume_id_and_end_are_saved() {
    let (mut store, _, _, session) = populated();
    store.save_resume_id(session.id, "8f1c").unwrap();
    store.end_session(session.id, at(99)).unwrap();
    let stored = &store.snapshot().unwrap().sessions[0];
    assert_eq!(stored.resume_id.as_deref(), Some("8f1c"));
    assert_eq!(stored.ended_at, Some(at(99)));
    assert!(matches!(
        store.end_session(SessionId::new(), at(1)),
        Err(StoreError::NotFound(_))
    ));
}

#[test]
fn samples_come_back_in_time_order_from_a_starting_point() {
    let (mut store, _, _, session) = populated();
    for (time, memory) in [(30, 300), (10, 100), (20, 200)] {
        store
            .record_sample(&ResourceSample {
                session: session.id,
                at: at(time),
                cpu_permille: 250,
                memory_bytes: memory,
            })
            .unwrap();
    }
    let memory: Vec<u64> = store
        .samples(session.id, at(20))
        .unwrap()
        .iter()
        .map(|sample| sample.memory_bytes)
        .collect();
    assert_eq!(memory, [200, 300]);
}

#[test]
fn a_corrupt_value_is_reported_not_replaced() {
    let (store, _, _, session) = populated();
    store
        .conn
        .execute(
            "UPDATE sessions SET status = '{\"state\":\"dancing\"}' WHERE id = ?1",
            [session.id.to_string()],
        )
        .unwrap();
    assert!(matches!(store.snapshot(), Err(StoreError::Invalid(_))));
}

// Handle

#[tokio::test]
async fn the_handle_runs_calls_on_the_store_thread() {
    let handle = StoreHandle::spawn(Store::open_in_memory().unwrap()).unwrap();
    let project = project("/src/app");
    let inserted = project.clone();
    handle
        .call(move |store| store.insert_project(&inserted))
        .await
        .unwrap()
        .unwrap();
    let snapshot = handle
        .call(|store| store.snapshot())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(snapshot.projects, [project]);
}

#[tokio::test]
async fn a_panicking_call_does_not_stop_the_store() {
    let handle = StoreHandle::spawn(Store::open_in_memory().unwrap()).unwrap();
    let panicked = handle.call(|_| -> () { panic!("bug") }).await;
    assert!(panicked.is_err());
    assert!(handle.call(|store| store.schema_version()).await.is_ok());
}

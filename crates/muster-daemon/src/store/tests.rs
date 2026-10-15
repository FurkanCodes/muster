use std::path::PathBuf;

use muster_core::{
    Project, ProjectId, Session, SessionId, SessionStatus, Timestamp, WaitReason, Workspace,
    WorkspaceId, WorkspaceKind,
};
use rusqlite::Connection;

use super::migrations::{MIGRATIONS, latest_version};
use super::{ResourceSample, RunProgress, RunUpdate, StatusChange, Store, StoreError, StoreHandle};

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
        pull_request: None,
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
        status_since: at(3),
        activity: None,
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
fn sessions_written_by_the_first_schema_survive_the_upgrade() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("muster.db");
    {
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(MIGRATIONS[0]).unwrap();
        conn.pragma_update(None, "user_version", 1).unwrap();
        conn.execute_batch(
            "INSERT INTO projects VALUES ('prj_01M4NG2BJY5GJ7C4GHEV9ZADAN', 'app', '/src/app', 1);
             INSERT INTO workspaces VALUES ('wsp_01M4NG2HX3A9RJ9SNANGSYT0DF',
                 'prj_01M4NG2BJY5GJ7C4GHEV9ZADAN', 'worktree', '/w/app/x', 'x', 2, NULL);
             INSERT INTO sessions VALUES ('ses_01M4NG33FANFNPXKH5FFNWYTZA',
                 'wsp_01M4NG2HX3A9RJ9SNANGSYT0DF', 'claude', 'Claude Code',
                 '{\"state\":\"finished\"}', 1, 'ea25847c', 3, NULL);",
        )
        .unwrap();
    }
    let store = Store::open(&path).unwrap();
    let sessions = store.snapshot().unwrap().sessions;
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].status, SessionStatus::Finished);
    assert!(sessions[0].unseen);
    assert_eq!(sessions[0].resume_id.as_deref(), Some("ea25847c"));
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
fn a_session_s_status_is_dated_from_when_it_came_to_it() {
    let (mut store, _, _, session) = populated();
    let since = |store: &Store| store.snapshot().unwrap().sessions[0].status_since;
    // None kept yet: its start.
    assert_eq!(since(&store), at(3));
    let record = |store: &mut Store, status, unseen, when| {
        store
            .record_status(session.id, status, unseen, at(when))
            .unwrap();
    };
    record(&mut store, SessionStatus::Running, false, 10);
    record(&mut store, SessionStatus::Finished, true, 20);
    // Looked at: the unseen mark goes, the status and its date stay.
    record(&mut store, SessionStatus::Finished, false, 30);
    assert_eq!(since(&store), at(20));
    record(&mut store, SessionStatus::Running, false, 40);
    record(&mut store, SessionStatus::Finished, false, 50);
    assert_eq!(since(&store), at(50), "a new run of the same status");
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
fn a_removed_session_leaves_the_snapshot_and_keeps_its_history() {
    let (mut store, _, _, session) = populated();
    store
        .record_status(session.id, SessionStatus::Running, false, at(10))
        .unwrap();
    store.remove_session(session.id, at(20)).unwrap();
    assert!(store.snapshot().unwrap().sessions.is_empty());
    assert_eq!(store.status_history(session.id).unwrap().len(), 1);
    assert!(matches!(
        store.remove_session(session.id, at(30)),
        Err(StoreError::NotFound(_))
    ));
}

#[test]
fn a_reopened_session_has_no_end() {
    let (mut store, _, _, session) = populated();
    store.end_session(session.id, at(99)).unwrap();
    store.reopen_session(session.id).unwrap();
    assert_eq!(store.snapshot().unwrap().sessions[0].ended_at, None);
    assert!(matches!(
        store.reopen_session(SessionId::new()),
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

// Runs

fn run(project: ProjectId) -> (muster_core::Run, muster_state::run::RunMachine) {
    let machine = muster_state::run::RunMachine::new(muster_state::run::Limits::default());
    let run = muster_core::Run {
        id: muster_core::RunId::new(),
        project,
        ticket: muster_core::Ticket {
            number: 42,
            title: "Fix the login redirect".to_owned(),
            url: "https://github.com/o/app/issues/42".to_owned(),
            body: "Users land on /404.".to_owned(),
            labels: vec!["bug".to_owned()],
            author: Some("someone".to_owned()),
        },
        branch: "muster/issue-42-fix-the-login-redirect".to_owned(),
        stage: muster_core::RunStage::Queued,
        attempt: 0,
        max_attempts: 3,
        reason: None,
        pr: None,
        cost_micros: None,
        created_at: at(5),
        ended_at: None,
    };
    (run, machine)
}

#[test]
fn a_run_is_stored_with_its_machine_and_its_timeline() {
    use muster_core::{PullRequest, RunStage, RunTransition};
    use muster_state::run::RunEvent;

    let (mut store, project, _, _) = populated();
    let (stored_run, mut machine) = run(project.id);
    store.insert_run(&stored_run, &machine).unwrap();
    assert_eq!(
        store.snapshot().unwrap().runs,
        std::slice::from_ref(&stored_run)
    );

    let now = std::time::Instant::now();
    machine.step(RunEvent::SlotGranted, now);
    let first = RunTransition {
        from: RunStage::Queued,
        to: RunStage::Provisioning,
        reason: "there is room to run".to_owned(),
        at: at(6),
    };
    let saved = machine.save(now);
    store
        .record_run_transition(
            stored_run.id,
            &first,
            &RunUpdate {
                stage: RunStage::Provisioning,
                reason: None,
                pr: None,
                machine: &saved,
                ended_at: None,
            },
        )
        .unwrap();
    let pr = PullRequest {
        number: 17,
        url: "https://github.com/o/app/pull/17".to_owned(),
    };
    machine.step(RunEvent::CostReported { micros: 250_000 }, now);
    store
        .save_run(
            stored_run.id,
            &RunProgress {
                machine: &machine.save(now),
                pr: Some(&pr),
                agent_session: Some("agent-1"),
                container: Some("c-1"),
            },
        )
        .unwrap();
    let second = RunTransition {
        from: RunStage::Provisioning,
        to: RunStage::Cancelled,
        reason: "you cancelled it".to_owned(),
        at: at(7),
    };
    store
        .record_run_transition(
            stored_run.id,
            &second,
            &RunUpdate {
                stage: RunStage::Cancelled,
                reason: None,
                pr: Some(&pr),
                machine: &machine.save(now),
                ended_at: Some(at(7)),
            },
        )
        .unwrap();

    let found = store.run(stored_run.id).unwrap().unwrap();
    assert_eq!(found.run.stage, RunStage::Cancelled);
    assert_eq!(found.run.pr, Some(pr));
    assert_eq!(found.run.cost_micros, Some(250_000));
    assert_eq!(found.run.ended_at, Some(at(7)));
    assert_eq!(found.run.ticket, stored_run.ticket);
    assert_eq!(found.machine.generation(), machine.generation());
    assert_eq!(found.agent_session.as_deref(), Some("agent-1"));
    assert_eq!(found.container.as_deref(), Some("c-1"));
    assert_eq!(store.run_history(stored_run.id).unwrap(), [first, second]);
    let progress = RunProgress {
        machine: &machine,
        pr: None,
        agent_session: None,
        container: None,
    };
    assert!(matches!(
        store.save_run(muster_core::RunId::new(), &progress),
        Err(StoreError::NotFound(_))
    ));
}

#[test]
fn a_project_s_runs_go_with_it() {
    let (mut store, project, _, _) = populated();
    let (stored_run, machine) = run(project.id);
    store.insert_run(&stored_run, &machine).unwrap();
    store.delete_project(project.id).unwrap();
    assert!(store.runs().unwrap().is_empty());
    assert!(store.run_history(stored_run.id).unwrap().is_empty());
}

#[test]
fn a_run_s_container_is_a_workspace_of_its_project_and_goes_with_the_run() {
    let (mut store, project, _, _) = populated();
    let (stored_run, machine) = run(project.id);
    store.insert_run(&stored_run, &machine).unwrap();
    let container = Workspace {
        id: WorkspaceId::new(),
        project: project.id,
        kind: WorkspaceKind::Container {
            run: stored_run.id,
            branch: stored_run.branch.clone(),
        },
        created_at: at(5),
        pull_request: None,
    };
    store.insert_workspace(&container).unwrap();
    let mut agent = session(container.id);
    agent.created_at = at(6);
    agent.status_since = at(6);
    store.insert_session(&agent).unwrap();

    let snapshot = store.snapshot().unwrap();
    assert!(snapshot.workspaces.contains(&container));
    assert!(snapshot.sessions.contains(&agent));
    assert_eq!(container.kind.host_path(), None);

    // A container of a run that is not stored is refused.
    let orphan = Workspace {
        id: WorkspaceId::new(),
        kind: WorkspaceKind::Container {
            run: muster_core::RunId::new(),
            branch: "x".to_owned(),
        },
        ..container.clone()
    };
    assert!(store.insert_workspace(&orphan).is_err());

    store.remove_workspace(container.id, at(9)).unwrap();
    let snapshot = store.snapshot().unwrap();
    assert!(!snapshot.workspaces.contains(&container));
    assert!(!snapshot.sessions.contains(&agent));
}

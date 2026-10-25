use std::path::PathBuf;

use muster_core::spaces::Member;
use muster_core::{
    Project, ProjectId, Session, SessionId, SessionStatus, Space, SpaceId, Timestamp, Workspace,
    WorkspaceId, WorkspaceKind,
};
use rusqlite::Connection;

use super::super::migrations::MIGRATIONS;
use super::super::{Store, StoreError};

fn at(millis: i64) -> Timestamp {
    Timestamp::from_millis(millis)
}

fn space(name: &str) -> Space {
    Space {
        id: SpaceId::new(),
        name: name.to_owned(),
        members: Vec::new(),
        created_at: at(1),
    }
}

fn session(workspace: Option<WorkspaceId>) -> Session {
    Session {
        id: SessionId::new(),
        workspace,
        harness: "shell".to_owned(),
        title: "zsh".to_owned(),
        status: SessionStatus::Fresh,
        unseen: false,
        resume_id: None,
        created_at: at(3),
        ended_at: None,
        status_since: at(3),
        activity: None,
        spend: None,
        tidy: None,
        beside: None,
        started_by: None,
        agent: None,
    }
}

/// A store with a project, its checkout, a terminal in it, a free
/// terminal, and a space holding the project and the free terminal.
fn populated() -> (Store, Project, Workspace, Session, Session, Space) {
    let mut store = Store::open_in_memory().unwrap();
    let project = Project {
        id: ProjectId::new(),
        name: "app".to_owned(),
        root: PathBuf::from("/src/app"),
        created_at: at(1),
    };
    let checkout = Workspace {
        id: WorkspaceId::new(),
        project: project.id,
        kind: WorkspaceKind::Checkout {
            path: project.root.clone(),
            branch: "main".to_owned(),
        },
        created_at: at(2),
        pull_request: None,
        changes: None,
        timeline: None,
        clashes: Vec::new(),
        stacked_on: None,
        ticket: None,
    };
    store.add_project(&project, &checkout).unwrap();
    let (inside, free) = (session(Some(checkout.id)), session(None));
    store.insert_session(&inside).unwrap();
    store.insert_session(&free).unwrap();
    let mut work = space("Work");
    store.insert_space(&work).unwrap();
    for member in [Member::Project(project.id), Member::Session(free.id)] {
        store.move_member(work.id, member).unwrap();
        work.members.push(member);
    }
    (store, project, checkout, inside, free, work)
}

#[test]
fn a_free_terminal_and_a_space_with_its_members_survive_reopening_arc_33() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("muster.db");
    let mut store = Store::open(&path, at(1)).unwrap();
    let free = session(None);
    store.insert_session(&free).unwrap();
    let mut work = space("Work");
    store.insert_space(&work).unwrap();
    store
        .move_member(work.id, Member::Session(free.id))
        .unwrap();
    // Again changes nothing.
    store
        .move_member(work.id, Member::Session(free.id))
        .unwrap();
    work.members.push(Member::Session(free.id));
    drop(store);

    let snapshot = Store::open(&path, at(1)).unwrap().snapshot().unwrap();
    assert_eq!(snapshot.sessions, [free]);
    assert_eq!(snapshot.spaces, [work]);
}

#[test]
fn a_member_moves_to_another_space_and_is_in_one_only_arc_33() {
    let (mut store, project, _, _, free, work) = populated();
    assert_eq!(store.spaces().unwrap(), std::slice::from_ref(&work));
    let home = Space {
        created_at: at(2),
        ..space("Home")
    };
    store.insert_space(&home).unwrap();
    store
        .move_member(home.id, Member::Project(project.id))
        .unwrap();
    let spaces = store.spaces().unwrap();
    assert_eq!(spaces[0].members, [Member::Session(free.id)]);
    assert_eq!(spaces[1].members, [Member::Project(project.id)]);
    store.rename_space(work.id, "Office").unwrap();
    assert_eq!(store.spaces().unwrap()[0].name, "Office");
    store
        .move_member(home.id, Member::Session(free.id))
        .unwrap();
    store.delete_space(work.id).unwrap();
    assert_eq!(store.spaces().unwrap().len(), 1);
    assert_eq!(store.snapshot().unwrap().sessions.len(), 2);
    assert!(matches!(
        store.delete_space(work.id),
        Err(StoreError::NotFound(_))
    ));
}

/// A store at schema 11, as `setup` leaves it, opened at the latest, with
/// the folder it is in.
fn migrated_from_11(setup: &str) -> (Store, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("muster.db");
    {
        let conn = Connection::open(&path).unwrap();
        for (version, migration) in (1_i64..).zip(&MIGRATIONS[..11]) {
            conn.execute_batch(migration).unwrap();
            conn.pragma_update(None, "user_version", version).unwrap();
        }
        conn.execute_batch(setup).unwrap();
    }
    (Store::open(&path, at(1)).unwrap(), dir)
}

const TWO_PROJECTS_AND_A_TERMINAL: &str =
    "INSERT INTO projects VALUES ('prj_01M4NG2BJY5GJ7C4GHEV9ZADAN', 'app', '/src/app', 5);
     INSERT INTO projects VALUES ('prj_01M4NG2BJY5GJ7C4GHEV9ZADAP', 'lib', '/src/lib', 7);
     INSERT INTO sessions (id, workspace_id, harness, title, status, unseen, created_at)
         VALUES ('ses_01M4NG33FANFNPXKH5FFNWYTZA', NULL, 'shell', 'zsh',
                 '{\"state\":\"fresh\"}', 0, 9);
     INSERT INTO sessions (id, workspace_id, harness, title, status, unseen, created_at,
                           removed_at)
         VALUES ('ses_01M4NG33FANFNPXKH5FFNWYTZB', NULL, 'shell', 'old',
                 '{\"state\":\"exited\",\"success\":true}', 0, 2, 3);";

#[test]
fn schema_12_puts_what_is_in_no_space_in_a_home_as_old_as_it_arc_33() {
    let (store, _dir) = migrated_from_11(TWO_PROJECTS_AND_A_TERMINAL);
    let spaces = store.spaces().unwrap();
    assert_eq!(spaces.len(), 1);
    assert_eq!(spaces[0].name, "Home");
    assert_eq!(spaces[0].created_at, at(5));
    // Oldest first; a terminal taken off the list stays out.
    assert_eq!(
        spaces[0].members,
        [
            Member::Project("prj_01M4NG2BJY5GJ7C4GHEV9ZADAN".parse().unwrap()),
            Member::Project("prj_01M4NG2BJY5GJ7C4GHEV9ZADAP".parse().unwrap()),
            Member::Session("ses_01M4NG33FANFNPXKH5FFNWYTZA".parse().unwrap()),
        ]
    );
}

#[test]
fn schema_12_keeps_one_in_several_spaces_in_the_first_and_uses_a_home_there_is() {
    let (store, _dir) = migrated_from_11(&format!(
        "{TWO_PROJECTS_AND_A_TERMINAL}
         INSERT INTO spaces VALUES ('spc_01M4NG2BJY5GJ7C4GHEV9ZADA1', 'Work', 1);
         INSERT INTO spaces VALUES ('spc_01M4NG2BJY5GJ7C4GHEV9ZADA2', 'home', 2);
         INSERT INTO space_members VALUES
             ('spc_01M4NG2BJY5GJ7C4GHEV9ZADA2', 'project', 'prj_01M4NG2BJY5GJ7C4GHEV9ZADAN');
         INSERT INTO space_members VALUES
             ('spc_01M4NG2BJY5GJ7C4GHEV9ZADA1', 'project', 'prj_01M4NG2BJY5GJ7C4GHEV9ZADAN');"
    ));
    let spaces = store.spaces().unwrap();
    assert_eq!(
        spaces.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
        ["Work", "home"],
        "no second Home is made"
    );
    assert_eq!(spaces[0].members.len(), 1, "app stays in Work, made first");
    assert_eq!(spaces[1].members.len(), 2, "lib and the terminal go home");
}

#[test]
fn a_name_is_one_space_s_whatever_its_case() {
    let (mut store, ..) = populated();
    let error = store.insert_space(&space("work")).unwrap_err();
    assert!(matches!(error, StoreError::Sqlite(_)), "{error}");
}

#[test]
fn a_member_leaves_its_spaces_with_it_arc_14() {
    let (mut store, project, _, inside, free, work) = populated();
    store.remove_session(free.id, at(9)).unwrap();
    assert_eq!(
        store.spaces().unwrap()[0].members,
        [Member::Project(project.id)]
    );
    // A project goes with its sessions, those its spaces held among them.
    let other = session(None);
    store.insert_session(&other).unwrap();
    store
        .move_member(work.id, Member::Session(inside.id))
        .unwrap();
    store
        .move_member(work.id, Member::Session(other.id))
        .unwrap();
    store.delete_project(project.id).unwrap();
    assert_eq!(
        store.spaces().unwrap()[0].members,
        [Member::Session(other.id)]
    );
}

#[test]
fn a_free_terminal_filed_under_a_workspace_leaves_its_spaces_arc_33() {
    let (mut store, project, checkout, _, free, _) = populated();
    store.file_session(free.id, checkout.id).unwrap();
    let snapshot = store.snapshot().unwrap();
    let filed = snapshot.sessions.iter().find(|s| s.id == free.id).unwrap();
    assert_eq!(filed.workspace, Some(checkout.id));
    assert_eq!(snapshot.spaces[0].members, [Member::Project(project.id)]);
    // Only a free one is filed.
    assert!(matches!(
        store.file_session(free.id, checkout.id),
        Err(StoreError::NotFound(_))
    ));
}

#[test]
fn sessions_and_their_history_survive_the_rebuild_of_their_table() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("muster.db");
    {
        let conn = Connection::open(&path).unwrap();
        for (version, migration) in (1_i64..).zip(&MIGRATIONS[..10]) {
            conn.execute_batch(migration).unwrap();
            conn.pragma_update(None, "user_version", version).unwrap();
        }
        conn.execute_batch(
            "INSERT INTO projects VALUES ('prj_01M4NG2BJY5GJ7C4GHEV9ZADAN', 'app', '/src/app', 1);
             INSERT INTO workspaces (id, project_id, kind, path, branch, created_at)
                 VALUES ('wsp_01M4NG2HX3A9RJ9SNANGSYT0DF', 'prj_01M4NG2BJY5GJ7C4GHEV9ZADAN',
                         'worktree', '/w/app/x', 'x', 2);
             INSERT INTO sessions
                 (id, workspace_id, harness, title, status, unseen, resume_id, created_at,
                  beside, started_by)
                 VALUES ('ses_01M4NG33FANFNPXKH5FFNWYTZA', 'wsp_01M4NG2HX3A9RJ9SNANGSYT0DF',
                         'claude', 'Fix', '{\"state\":\"finished\"}', 1, 'ea25847c', 3,
                         NULL, NULL);
             INSERT INTO status_changes (session_id, status, unseen, at)
                 VALUES ('ses_01M4NG33FANFNPXKH5FFNWYTZA', '{\"state\":\"finished\"}', 1, 4);",
        )
        .unwrap();
    }
    let store = Store::open(&path, at(1)).unwrap();
    let sessions = store.snapshot().unwrap().sessions;
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].resume_id.as_deref(), Some("ea25847c"));
    assert_eq!(store.status_history(sessions[0].id).unwrap().len(), 1);
}

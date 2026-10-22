//! Projects that are plain folders, not git repositories: sessions in the
//! folder, and no worktrees.

use std::path::PathBuf;

use crossterm::event::KeyCode;
use muster_core::protocol::{Delta, ServerFrame};
use muster_core::{Project, ProjectId, Timestamp, Workspace, WorkspaceId, WorkspaceKind};

use super::tests::{World, press, render, world};
use super::{Dialog, Mode};
use crate::model::Row;

/// The world with a second project, `notes`, a plain folder.
fn with_folder() -> (World, Project, Workspace) {
    let mut world = world();
    let project = Project {
        id: ProjectId::new(),
        name: "notes".to_owned(),
        root: PathBuf::from("/home/me/Desktop/notes"),
        created_at: Timestamp::from_millis(60),
    };
    let folder = Workspace {
        id: WorkspaceId::new(),
        project: project.id,
        kind: WorkspaceKind::Checkout {
            path: project.root.clone(),
            branch: String::new(),
        },
        created_at: Timestamp::from_millis(60),
        pull_request: None,
        changes: None,
        timeline: None,
        clashes: Vec::new(),
        stacked_on: None,
        ticket: None,
    };
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::ProjectUpserted(project.clone())));
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::WorkspaceUpserted(folder.clone())));
    (world, project, folder)
}

#[test]
fn a_folder_is_listed_as_not_a_git_repository() {
    let (mut world, project, folder) = with_folder();
    assert!(world.app.model.is_plain_folder(project.id));
    assert!(!world.app.model.is_plain_folder(world.project.id));
    let screen = render(&world.app);
    assert!(screen.contains("⌂ not a git repository"), "{screen}");

    world.app.selected = Some(Row::Project(project.id));
    let screen = render(&world.app);
    assert!(screen.contains("/home/me/Desktop/notes"), "{screen}");
    assert!(screen.contains("no worktrees"), "{screen}");
    assert!(!screen.contains("Worktrees"), "{screen}");
    assert!(screen.contains("⏎ session"), "{screen}");
    assert!(!screen.contains("i tickets"), "{screen}");

    world.app.selected = Some(Row::Worktree(folder.id));
    let screen = render(&world.app);
    assert!(screen.contains("own folder"), "{screen}");
    assert!(!screen.contains("Branch"), "{screen}");
    assert!(!screen.contains("w worktree"), "{screen}");
}

#[test]
fn enter_on_a_folder_starts_a_session_in_it() {
    let (mut world, project, folder) = with_folder();
    world.app.selected = Some(Row::Project(project.id));
    press(&mut world.app, KeyCode::Enter);
    let Mode::Dialog(Dialog::NewSession { workspace, .. }) = &world.app.mode else {
        panic!("expected the new-session dialog, got {:?}", world.app.mode);
    };
    assert_eq!(*workspace, folder.id);
    assert!(render(&world.app).contains("In notes"));
}

#[test]
fn a_folder_has_no_worktrees_and_says_so() {
    let (mut world, project, _) = with_folder();
    world.app.selected = Some(Row::Project(project.id));
    press(&mut world.app, KeyCode::Char('w'));
    assert_eq!(world.app.mode, Mode::Browse);
    let note = world.app.note.clone().expect("a note");
    assert!(
        note.error && note.text.contains("notes is a plain folder"),
        "{note:?}"
    );

    // A repository still gets its worktree dialog.
    world.app.selected = Some(Row::Project(world.project.id));
    press(&mut world.app, KeyCode::Char('w'));
    assert!(matches!(
        world.app.mode,
        Mode::Dialog(Dialog::NewWorktree { .. })
    ));
}

#[test]
fn a_folder_made_a_repository_shows_its_branch() {
    let (mut world, project, mut folder) = with_folder();
    folder.kind = WorkspaceKind::Checkout {
        path: project.root.clone(),
        branch: "main".to_owned(),
    };
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::WorkspaceUpserted(folder)));
    assert!(!world.app.model.is_plain_folder(project.id));
    let screen = render(&world.app);
    assert!(screen.contains("⌂ main"), "{screen}");
    assert!(!screen.contains("not a git repository"), "{screen}");
}

#[test]
fn the_add_dialog_takes_any_folder() {
    let (mut world, _, _) = with_folder();
    press(&mut world.app, KeyCode::Char('a'));
    assert!(render(&world.app).contains("or any folder"));
}

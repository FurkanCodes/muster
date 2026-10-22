//! Projects' own checkouts, and removing projects.

use std::path::PathBuf;

use crossterm::event::KeyCode;
use muster_core::protocol::{Command, Delta, ServerFrame};
use muster_core::{Timestamp, Workspace, WorkspaceId, WorkspaceKind};

use super::tests::{World, command, press, render, world};
use super::{Confirmed, Dialog, Mode};
use crate::model::Row;

/// The world with its project's own checkout, on `main`.
fn with_checkout() -> (World, Workspace) {
    let mut world = world();
    let checkout = Workspace {
        id: WorkspaceId::new(),
        project: world.project.id,
        kind: WorkspaceKind::Checkout {
            path: PathBuf::from("/src/app"),
            branch: "main".to_owned(),
        },
        // Added after the worktree, as for a project from before checkouts.
        created_at: Timestamp::from_millis(50),
        pull_request: None,
        changes: None,
        timeline: None,
        clashes: Vec::new(),
        stacked_on: None,
        ticket: None,
    };
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::WorkspaceUpserted(
            checkout.clone(),
        )));
    (world, checkout)
}

#[test]
fn the_checkout_is_listed_first_and_marked() {
    let (world, checkout) = with_checkout();
    let rows = world.app.model.rows();
    assert_eq!(
        rows[..3],
        [
            Row::Project(world.project.id),
            Row::Worktree(checkout.id),
            Row::Worktree(world.worktree.id),
        ]
    );
    let screen = render(&world.app);
    assert!(screen.contains("⌂ main"), "{screen}");
    assert!(screen.contains("⎇ work"), "{screen}");
}

#[test]
fn n_on_a_project_starts_a_session_in_its_checkout() {
    let (mut world, checkout) = with_checkout();
    world.app.selected = Some(Row::Project(world.project.id));
    press(&mut world.app, KeyCode::Char('n'));
    let Mode::Dialog(Dialog::NewSession { workspace, .. }) = &world.app.mode else {
        panic!("expected the new-session dialog, got {:?}", world.app.mode);
    };
    assert_eq!(*workspace, checkout.id);
}

#[test]
fn the_checkout_cannot_be_removed_and_says_how_to_stop_tracking() {
    let (mut world, checkout) = with_checkout();
    world.app.selected = Some(Row::Worktree(checkout.id));
    press(&mut world.app, KeyCode::Char('x'));
    assert_eq!(world.app.mode, Mode::Browse);
    let note = world.app.note.clone().expect("a note");
    assert!(
        note.error && note.text.contains("never removes"),
        "{note:?}"
    );
    let screen = render(&world.app);
    assert!(screen.contains("own checkout"), "{screen}");
}

#[test]
fn x_on_a_project_asks_and_then_removes_it() {
    let (mut world, _) = with_checkout();
    world.app.selected = Some(Row::Project(world.project.id));
    press(&mut world.app, KeyCode::Char('x'));
    assert!(matches!(
        world.app.mode,
        Mode::Dialog(Dialog::Confirm {
            then: Confirmed::RemoveProject(_),
            ..
        })
    ));
    assert!(render(&world.app).contains("touched."));
    let (_, sent) = command(&press(&mut world.app, KeyCode::Char('y')));
    assert_eq!(
        sent,
        Command::RemoveProject {
            project: world.project.id
        }
    );

    world
        .app
        .on_frame(ServerFrame::Delta(Delta::ProjectRemoved {
            id: world.project.id,
        }));
    assert!(world.app.model.rows().is_empty());
    assert!(world.app.model.session(world.session.id).is_none());
    assert_eq!(world.app.selected, None);
    assert!(render(&world.app).contains("No projects yet"));
}

#[test]
fn a_branch_switched_under_a_workspace_is_shown() {
    let (mut world, mut checkout) = with_checkout();
    checkout.kind = WorkspaceKind::Checkout {
        path: PathBuf::from("/src/app"),
        branch: "detached at 0123456".to_owned(),
    };
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::WorkspaceUpserted(checkout)));
    assert!(render(&world.app).contains("⌂ detached at 0123456"));
}

#[test]
fn a_long_path_scrolls_so_the_typing_end_stays_in_view() {
    let long = format!("/{}/end-of-path", "very-long-directory".repeat(6));
    let mut app = super::App::new(PathBuf::from(&long), super::tests::SIZE);
    app.on_connected("0.1.0".to_owned());
    app.on_frame(ServerFrame::Snapshot(
        muster_core::protocol::Snapshot::default(),
    ));
    press(&mut app, KeyCode::Char('a'));
    let screen = render(&app);
    assert!(screen.contains("end-of-path"), "{screen}");
    // The start scrolled out of view, rather than the end being cut off.
    assert!(!screen.contains(&format!("/{}", "very-long-directory".repeat(3))));
}

#[test]
fn a_long_question_wraps_instead_of_being_cut_off() {
    let (mut world, _) = with_checkout();
    world.app.selected = Some(Row::Project(world.project.id));
    press(&mut world.app, KeyCode::Char('x'));
    let screen = render(&world.app);
    assert!(screen.contains("Stop tracking app?"), "{screen}");
    assert!(screen.contains("touched."), "{screen}");
    assert!(screen.contains("y yes"), "{screen}");
}

#[test]
fn a_long_title_gives_way_so_the_status_stays_in_view() {
    let mut world = world();
    let mut long = world.session.clone();
    long.title = "#11419 A very long issue title that goes on and on and on".to_owned();
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::SessionUpserted(long)));
    // On two lines, the status stays under the title cut short.
    let screen = render(&world.app);
    let lines: Vec<&str> = screen.lines().collect();
    let at = lines
        .iter()
        .position(|line| line.contains("#11419"))
        .expect("the session's row");
    assert!(lines[at].contains('…'), "{screen}");
    assert!(lines[at + 1].contains("idle"), "{screen}");
    // On one, it stays beside it.
    super::tests::one_line(&mut world.app);
    let screen = render(&world.app);
    let row = screen
        .lines()
        .find(|line| line.contains("#11419"))
        .expect("the session's row");
    assert!(row.contains("…  idle"), "{row}");
}

#[test]
fn control_characters_in_a_title_are_never_drawn() {
    let mut world = world();
    let mut hostile = world.session.clone();
    hostile.title = "evil\u{1b}]0;owned\u{7}\u{1b}[2J title".to_owned();
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::SessionUpserted(hostile)));
    world.app.selected = Some(Row::Session(world.session.id));
    let screen = render(&world.app);
    assert!(
        !screen.contains('\u{1b}') && !screen.contains('\u{7}'),
        "{screen:?}"
    );
    assert!(screen.contains("evil]0;owned[2J title"), "{screen}");
}

/// The world with a run on #42 taken over, and its container.
fn with_container() -> (World, Workspace) {
    let mut world = world();
    let run = muster_core::Run {
        id: muster_core::RunId::new(),
        project: world.project.id,
        ticket: muster_core::Ticket {
            number: 42,
            title: "Fix the login redirect".to_owned(),
            url: "https://github.com/o/app/issues/42".to_owned(),
            body: String::new(),
            labels: Vec::new(),
            author: None,
        },
        branch: "muster/issue-42-fix".to_owned(),
        stage: muster_core::RunStage::TakenOver,
        attempt: 1,
        max_attempts: 3,
        reason: None,
        pr: None,
        cost_micros: None,
        created_at: Timestamp::from_millis(60),
        ended_at: None,
    };
    let container = Workspace {
        id: WorkspaceId::new(),
        project: world.project.id,
        kind: WorkspaceKind::Container {
            run: run.id,
            branch: run.branch.clone(),
        },
        created_at: Timestamp::from_millis(1),
        pull_request: None,
        changes: None,
        timeline: None,
        clashes: Vec::new(),
        stacked_on: None,
        ticket: None,
    };
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::RunUpserted(run)));
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::WorkspaceUpserted(
            container.clone(),
        )));
    (world, container)
}

#[test]
fn a_run_s_container_follows_the_worktrees_with_its_issue_and_stage() {
    let (mut world, container) = with_container();
    let rows = world.app.model.rows();
    let at = |row| rows.iter().position(|candidate| *candidate == row).unwrap();
    assert!(at(Row::Worktree(container.id)) > at(Row::Worktree(world.worktree.id)));
    world.app.selected = Some(Row::Worktree(container.id));
    let screen = render(&world.app);
    assert!(screen.contains("⧉ #42 taken over  muster/iss"), "{screen}");
    assert!(
        screen.contains("The container of the run on #42"),
        "{screen}"
    );
    assert!(!screen.contains("x remove"), "{screen}");
}

#[test]
fn a_session_in_a_run_s_container_starts_as_a_shell_and_it_is_not_removed() {
    let (mut world, container) = with_container();
    world.app.selected = Some(Row::Worktree(container.id));
    press(&mut world.app, KeyCode::Char('n'));
    let Mode::Dialog(Dialog::NewSession { harness, .. }) = &world.app.mode else {
        panic!("expected the new-session dialog, got {:?}", world.app.mode);
    };
    assert_eq!(*harness, super::HarnessChoice::Shell);
    press(&mut world.app, KeyCode::Esc);
    press(&mut world.app, KeyCode::Char('x'));
    assert_eq!(world.app.mode, Mode::Browse);
    let note = world.app.note.clone().expect("a note");
    assert!(
        note.error && note.text.contains("cancel the run"),
        "{note:?}"
    );
}

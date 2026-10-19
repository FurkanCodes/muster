//! The sessions list at a glance: each project's and worktree's line
//! folds away what is under it, saying how many sessions it hides and how
//! the most urgent of them stands; unfolded, only the sessions' own lines
//! say how they stand (`PRD` §6 Sessions).

use crossterm::event::{KeyCode, MouseButton, MouseEventKind};
use muster_core::protocol::{Delta, ServerFrame};
use muster_core::{Session, SessionStatus, WaitReason};

use super::pointer_tests::mouse;
use super::tests::{World, press, render, world};
use super::{Mode, View};
use crate::model::Row;
use crate::model::tests::{project, session, worktree};

/// The list's line that holds `text`, without the pane beside it: the
/// world's list is 30 columns, as narrow as it goes.
fn line_with(screen: &str, text: &str) -> String {
    let line: String = screen
        .lines()
        .find(|line| line.contains(text))
        .unwrap_or_else(|| panic!("no line with {text}:\n{screen}"))
        .chars()
        .take(30)
        .collect();
    line.trim_end_matches('│').to_owned()
}

fn upsert(world: &mut World, session: Session) {
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::SessionUpserted(session)));
}

/// The world's session as `status`, and as seen or not.
fn shell_as(world: &mut World, status: SessionStatus, unseen: bool) {
    let mut shell = world.session.clone();
    shell.status = status;
    shell.unseen = unseen;
    upsert(world, shell);
}

#[test]
fn unfolded_only_a_session_s_own_line_shows_its_mark() {
    let mut world = world();
    world.app.motion = false;
    shell_as(&mut world, SessionStatus::Running, false);
    let screen = render(&world.app);
    assert!(screen.contains("● • Shell"), "{screen}");
    assert!(line_with(&screen, "▾ app").trim_end().ends_with("app"));
    assert!(line_with(&screen, "⎇ work").trim_end().ends_with("work"));
}

#[test]
fn a_folded_project_and_worktree_show_their_most_urgent_session_in_its_mark() {
    let mut world = world();
    world.app.motion = false;
    world.app.fold(Row::Worktree(world.worktree.id));
    // Nothing at work: nothing to say but what it hides.
    let screen = render(&world.app);
    assert!(line_with(&screen, "⎇ work").contains("work 1 session"));

    // At work: its mark.
    shell_as(&mut world, SessionStatus::Running, false);
    let screen = render(&world.app);
    assert!(line_with(&screen, "⎇ work").contains("work ●"), "{screen}");
    assert!(line_with(&screen, "▾ app").trim_end().ends_with("app"));

    // Finished while nobody looked, and another asking: the question
    // first, as Needs you goes.
    shell_as(&mut world, SessionStatus::Finished, true);
    let screen = render(&world.app);
    assert!(line_with(&screen, "⎇ work").contains("work ✓•"), "{screen}");
    let mut asking = session(world.worktree.id, "Claude", 4);
    asking.status = SessionStatus::NeedsYou {
        reason: WaitReason::Permission,
    };
    upsert(&mut world, asking);
    let screen = render(&world.app);
    assert!(line_with(&screen, "⎇ work").contains("work ▲"), "{screen}");
    world.app.fold(Row::Project(world.project.id));
    let screen = render(&world.app);
    assert!(line_with(&screen, "▸ app").contains("app ▲"), "{screen}");
    world.app.fold(Row::Project(world.project.id));

    // In the session's own colour.
    let buffer = super::look_tests::buffer(&world.app);
    let row = (0..buffer.area.height)
        .find(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, *y)].symbol())
                .collect::<String>()
                .contains("⎇ work")
        })
        .unwrap();
    let mark = (0..buffer.area.width)
        .find(|x| buffer[(*x, row)].symbol() == "▲")
        .unwrap();
    assert_eq!(
        buffer[(mark, row)].fg,
        crate::view::theme::of(&world.app).waiting
    );
}

#[test]
fn a_finished_session_that_was_seen_says_nothing_on_its_worktree() {
    let mut world = world();
    shell_as(&mut world, SessionStatus::Finished, false);
    let screen = render(&world.app);
    assert!(!line_with(&screen, "⎇ work").contains('✓'), "{screen}");
}

#[test]
fn e_folds_a_worktree_which_keeps_its_mark_and_says_what_it_hides() {
    let mut world = world();
    world.app.motion = false;
    shell_as(&mut world, SessionStatus::Running, false);
    world.app.selected = Some(Row::Worktree(world.worktree.id));
    assert!(
        world
            .app
            .actions_here()
            .iter()
            .any(|here| here.what == "fold"),
        "the menu and the hints offer it"
    );

    press(&mut world.app, KeyCode::Char('e'));
    let screen = render(&world.app);
    assert!(!screen.contains("Shell"), "{screen}");
    let line = line_with(&screen, "⎇ work");
    assert!(line.contains("▸ ⎇ work ● 1 session"), "{line}");
    assert!(
        world
            .app
            .actions_here()
            .iter()
            .any(|here| here.what == "unfold"),
        "{screen}"
    );

    press(&mut world.app, KeyCode::Char('e'));
    let screen = render(&world.app);
    assert!(screen.contains("● • Shell"), "{screen}");
    assert!(!line_with(&screen, "⎇ work").contains('●'), "{screen}");

    // On a project's line, where the hints have room, they say so.
    world.app.selected = Some(Row::Project(world.project.id));
    assert!(render(&world.app).contains("e fold"));
}

#[test]
fn e_on_a_session_folds_its_worktree_and_selects_it() {
    let mut world = world();
    world.app.selected = Some(Row::Session(world.session.id));
    press(&mut world.app, KeyCode::Char('e'));
    assert!(world.app.is_folded(Row::Worktree(world.worktree.id)));
    assert_eq!(
        world.app.selected,
        Some(Row::Worktree(world.worktree.id)),
        "the selection rests on a line drawn"
    );
}

#[test]
fn a_folded_project_hides_its_worktrees_and_the_selection_skips_them() {
    let mut world = world();
    let other = project("web");
    let fix = worktree(other.id, "fix");
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::ProjectUpserted(other.clone())));
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::WorkspaceUpserted(fix.clone())));
    upsert(&mut world, session(fix.id, "Other", 5));
    let second = session(world.worktree.id, "Second", 6);
    upsert(&mut world, second);

    // Folded with a session in it selected: the project is selected.
    world.app.selected = Some(Row::Session(world.session.id));
    world.app.fold(Row::Project(world.project.id));
    assert_eq!(world.app.selected, Some(Row::Project(world.project.id)));
    let screen = render(&world.app);
    assert!(!screen.contains("⎇ work"), "{screen}");
    assert!(
        line_with(&screen, "▸ app").contains("app 2 sessions"),
        "{screen}"
    );

    // ↓ goes from it straight to the next project.
    press(&mut world.app, KeyCode::Down);
    assert_eq!(world.app.selected, Some(Row::Project(other.id)));
    press(&mut world.app, KeyCode::Up);
    assert_eq!(world.app.selected, Some(Row::Project(world.project.id)));
}

#[test]
fn a_click_on_a_triangle_folds_and_unfolds_its_line() {
    let mut world = world();
    let window = ratatui::layout::Rect::new(0, 0, 100, 30);
    let list = crate::view::browse_areas(&world.app, window).list;
    // The project's line is the list's first; its worktree's the second.
    let click = |world: &mut World, column: u16, line: u16| {
        let at = (list.x + 1 + column, list.y + 1 + line);
        world
            .app
            .on_mouse(mouse(MouseEventKind::Down(MouseButton::Left), at.0, at.1));
    };
    click(&mut world, 2, 1);
    assert!(world.app.is_folded(Row::Worktree(world.worktree.id)));
    assert_eq!(world.app.selected, Some(Row::Worktree(world.worktree.id)));
    click(&mut world, 2, 1);
    assert!(!world.app.is_folded(Row::Worktree(world.worktree.id)));

    click(&mut world, 1, 0);
    assert!(world.app.is_folded(Row::Project(world.project.id)));
    // Elsewhere on the line, a click only selects.
    click(&mut world, 8, 0);
    assert!(world.app.is_folded(Row::Project(world.project.id)));
    click(&mut world, 1, 0);
    assert!(!world.app.is_folded(Row::Project(world.project.id)));
}

#[test]
fn a_worktree_with_no_session_has_nothing_to_fold() {
    let mut world = world();
    let empty = worktree(world.project.id, "empty");
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::WorkspaceUpserted(empty.clone())));
    world.app.selected = Some(Row::Worktree(empty.id));
    let screen = render(&world.app);
    assert!(line_with(&screen, "⎇ empty").contains("█   ⎇ empty"));
    assert!(!screen.contains("e fold"), "{screen}");
    press(&mut world.app, KeyCode::Char('e'));
    assert!(!world.app.is_folded(Row::Worktree(empty.id)));
    assert!(render(&world.app).contains("No sessions here to fold away."));
}

#[test]
fn a_session_opened_for_the_person_unfolds_what_hides_it() {
    let mut world = world();
    world.app.fold(Row::Project(world.project.id));
    world.app.fold(Row::Worktree(world.worktree.id));
    // Picked in the notch: it opens beside the list, its line in view.
    world.app.on_frame(ServerFrame::Reveal {
        session: world.session.id,
    });
    assert_eq!(world.app.mode, Mode::Terminal(world.session.id));
    assert_eq!(world.app.view, View::Sessions);
    assert!(!world.app.is_folded(Row::Project(world.project.id)));
    assert!(!world.app.is_folded(Row::Worktree(world.worktree.id)));
    assert!(world.app.rows().contains(&Row::Session(world.session.id)));
}

#[test]
fn a_fold_goes_with_its_project() {
    let mut world = world();
    world.app.fold(Row::Project(world.project.id));
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::ProjectRemoved {
            id: world.project.id,
        }));
    assert!(!world.app.is_folded(Row::Project(world.project.id)));
}

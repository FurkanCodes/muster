//! Stacked work (`PRODUCT.md` §6, `PRD-14`): a ticket's new worktree on
//! top of another's branch, what a stacked worktree's details say, and `u`
//! moving a branch only once the person says yes.

use crossterm::event::KeyCode;
use muster_core::protocol::{Command, Delta, Reply, ServerFrame};
use muster_core::stacks::Stacked;
use muster_core::{PullRequestState, PullRequestStatus, Workspace};

use super::super::tests::{World, command, press, render, world};
use super::super::tickets_tests::{commands, key, on_tickets};
use super::super::{Dialog, Mode};
use crate::model::Row;
use crate::model::tests::worktree;

#[test]
fn a_ticket_s_new_worktree_can_be_made_on_top_of_another_s_branch() {
    let mut world = on_tickets();
    press(&mut world.app, KeyCode::Enter);
    let screen = render(&world.app);
    assert!(screen.contains("On top of ◂ its base ▸"), "{screen}");
    assert!(screen.contains("←→ on top of"), "{screen}");
    press(&mut world.app, KeyCode::Right);
    assert!(render(&world.app).contains("On top of ◂ ⎇ work ▸"));
    // Round through the base again, and back.
    press(&mut world.app, KeyCode::Right);
    assert!(render(&world.app).contains("On top of ◂ its base ▸"));
    press(&mut world.app, KeyCode::Left);
    let actions = press(&mut world.app, KeyCode::Enter);
    assert!(matches!(
        &commands(&actions)[..],
        [Command::StartFromIssue {
            key: started,
            harness,
            on: Some(on),
            ..
        }] if *started == key(42) && harness == "claude" && *on == world.worktree.id
    ));
}

#[test]
fn an_older_daemon_is_never_asked_to_stack() {
    let mut world = on_tickets();
    world.app.daemon_protocol = muster_core::tidy::TIDY_PROTOCOL;
    press(&mut world.app, KeyCode::Enter);
    assert!(!render(&world.app).contains("On top of"));
    // ← → choose the agent, as before.
    press(&mut world.app, KeyCode::Right);
    let actions = press(&mut world.app, KeyCode::Enter);
    assert!(matches!(
        &commands(&actions)[..],
        [Command::StartFromIssue { harness, on: None, .. }] if harness == "codex"
    ));
    world.app.selected = Some(Row::Worktree(world.worktree.id));
    world.app.view = super::super::View::Sessions;
    assert!(press(&mut world.app, KeyCode::Char('u')).is_empty());
    assert!(render(&world.app).contains("`muster reload`"));
}

/// The world with a second worktree on `child`, stacked on the world's
/// worktree's branch; the parent's pull request in `state`.
fn stacked(state: Option<PullRequestState>) -> (World, Workspace) {
    let mut world = world();
    let mut parent = world.worktree.clone();
    parent.pull_request = state.map(|state| PullRequestStatus {
        number: 7,
        url: "https://github.com/o/app/pull/7".to_owned(),
        state,
        checks: None,
        review: None,
    });
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::WorkspaceUpserted(parent)));
    let mut child = worktree(world.project.id, "child");
    child.stacked_on = Some(Stacked {
        branch: "work".to_owned(),
        at: "a".repeat(40),
    });
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::WorkspaceUpserted(child.clone())));
    world.app.selected = Some(Row::Worktree(child.id));
    (world, child)
}

#[test]
fn a_stacked_worktree_says_on_what_and_u_asks_before_moving_it() {
    let (mut world, child) = stacked(Some(PullRequestState::Open));
    let screen = render(&world.app);
    assert!(screen.contains("Stacked on ⎇ work"), "{screen}");
    assert!(!screen.contains("it has merged"), "{screen}");

    assert!(press(&mut world.app, KeyCode::Char('u')).is_empty());
    assert!(matches!(
        world.app.mode,
        Mode::Dialog(Dialog::Confirm { .. })
    ));
    let Mode::Dialog(Dialog::Confirm { question, .. }) = &world.app.mode else {
        panic!("{:?}", world.app.mode);
    };
    assert!(
        question.starts_with("Move child onto the newest of ⎇ work?"),
        "{question}"
    );
    // No: nothing is sent.
    assert!(press(&mut world.app, KeyCode::Char('n')).is_empty());
    press(&mut world.app, KeyCode::Char('u'));
    let (req_id, asked) = command(&press(&mut world.app, KeyCode::Char('y')));
    assert_eq!(
        asked,
        Command::MoveOnto {
            workspace: child.id
        }
    );
    world.app.on_frame(ServerFrame::Reply {
        req_id,
        outcome: Ok(Reply::Moved {
            onto: "work".to_owned(),
            commits: 2,
            entry: 4,
        }),
    });
    let screen = render(&world.app);
    assert!(
        screen.contains("child is on work now, its 2 commits replayed there; z on #4 in d puts it"),
        "{screen}"
    );
}

#[test]
fn once_what_it_is_stacked_on_merges_u_moves_it_onto_its_base() {
    let (mut world, _) = stacked(Some(PullRequestState::Merged));
    assert!(
        render(&world.app)
            .contains("Stacked on ⎇ work · it has merged: u moves this onto its base")
    );
    press(&mut world.app, KeyCode::Char('u'));
    let Mode::Dialog(Dialog::Confirm { question, .. }) = &world.app.mode else {
        panic!("{:?}", world.app.mode);
    };
    assert!(
        question.starts_with(
            "Move child onto the newest of its base, as the branch it was stacked on has merged?"
        ),
        "{question}"
    );
}

#[test]
fn a_move_is_named_in_the_timeline() {
    let entry = muster_core::timeline::Entry {
        number: 4,
        kind: muster_core::timeline::EntryKind::Moved {
            onto: "origin/main".to_owned(),
        },
        started: muster_core::Timestamp::from_millis(1),
        ended: muster_core::Timestamp::from_millis(1),
        summary: None,
        head_before: None,
        head_after: None,
    };
    assert_eq!(
        crate::app::timeline::label(None, &entry),
        "Moved onto origin/main"
    );
}

//! Fewer false alarms, and a way back to the last (`ARCHITECTURE.md` §5a
//! Notifications): a finish is told only once it has stayed one a moment,
//! and `l` goes to the session told of last.

use crossterm::event::KeyCode;
use muster_core::protocol::{Delta, ServerFrame};
use muster_core::{SessionStatus, WaitReason};

use super::alarm::HOLD_STEPS;
use super::tests::{World, press, render, world};
use super::{Action, Mode, View};
use crate::model::Row;

fn status(world: &mut World, status: SessionStatus, unseen: bool) -> Vec<Action> {
    let session = world.session.id;
    world.app.on_frame(ServerFrame::Delta(Delta::SessionStatus {
        session,
        status,
        unseen,
        since: muster_core::Timestamp::from_millis(10),
    }))
}

fn notes(actions: &[Action]) -> Vec<String> {
    actions
        .iter()
        .filter_map(|action| match action {
            Action::Notify(text) => Some(text.clone()),
            _ => None,
        })
        .collect()
}

/// The world with its window out of focus, its session at work.
fn away() -> World {
    let mut world = world();
    world.app.on_focus(false);
    status(&mut world, SessionStatus::Running, false);
    world
}

#[test]
fn a_finish_is_told_once_it_has_stayed_one_a_moment() {
    let mut world = away();
    world.app.on_step(100);
    assert!(notes(&status(&mut world, SessionStatus::Finished, true)).is_empty());
    assert!(world.app.wants_steps(), "the hold asks for steps");
    world.app.on_step(100 + HOLD_STEPS - 1);
    assert!(world.app.due_news().is_empty(), "not yet");
    world.app.on_step(100 + HOLD_STEPS);
    assert_eq!(
        notes(&world.app.due_news()),
        ["Shell in app (work) finished"]
    );
    assert!(world.app.due_news().is_empty(), "told once");
}

#[test]
fn a_finish_at_work_again_by_then_is_dropped() {
    let mut world = away();
    status(&mut world, SessionStatus::Finished, true);
    world.app.on_step(5);
    status(&mut world, SessionStatus::Running, false);
    world.app.on_step(HOLD_STEPS);
    assert!(world.app.due_news().is_empty());
    assert!(!world.app.news_held());
    // Nor is there an alarm to go back to.
    press(&mut world.app, KeyCode::Char('l'));
    assert!(render(&world.app).contains("No session has needed you"));
}

#[test]
fn a_wait_is_told_at_once() {
    let mut world = away();
    let waiting = SessionStatus::NeedsYou {
        reason: WaitReason::Question,
    };
    assert_eq!(
        notes(&status(&mut world, waiting, true)),
        ["Shell in app (work) has a question for you"]
    );
}

#[test]
fn l_goes_to_the_session_told_of_last_whatever_hides_it() {
    let mut world = away();
    status(
        &mut world,
        SessionStatus::NeedsYou {
            reason: WaitReason::Permission,
        },
        true,
    );
    world.app.fold(Row::Project(world.project.id));
    press(&mut world.app, KeyCode::Char('3'));
    assert_eq!(world.app.view, View::Tickets);
    press(&mut world.app, KeyCode::Char('l'));
    assert_eq!(world.app.view, View::Sessions);
    assert_eq!(world.app.mode, Mode::Browse);
    assert_eq!(world.app.selected, Some(Row::Session(world.session.id)));
    assert!(world.app.rows().contains(&Row::Session(world.session.id)));
}

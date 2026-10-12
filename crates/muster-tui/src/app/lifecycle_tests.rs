//! Restarting and removing sessions, and what the person sees around them.

use crossterm::event::KeyCode;
use muster_core::protocol::{
    ClientFrame, Command, Delta, ErrorBody, ErrorCode, Reply, ServerFrame,
};
use muster_core::{Session, SessionStatus, Timestamp};

use super::tests::{PANE, SIZE, World, command, press, render, world};
use super::{Action, Link, Mode};
use crate::model::Row;

/// The world's session, ended, and selected in the list.
fn ended() -> World {
    let mut world = world();
    let mut session = world.session.clone();
    session.status = SessionStatus::Exited { success: true };
    session.ended_at = Some(Timestamp::from_millis(9));
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::SessionUpserted(session.clone())));
    world.session = session;
    world.app.selected = Some(Row::Session(world.session.id));
    world
}

/// Whether `actions` send no command (attaching to the selection may
/// still happen).
fn no_command(actions: &[Action]) -> bool {
    !actions
        .iter()
        .any(|action| matches!(action, Action::Send(ClientFrame::Command { .. })))
}

fn upsert(world: &mut World, session: &Session) -> Vec<Action> {
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::SessionUpserted(session.clone())))
}

#[test]
fn r_restarts_an_ended_session_and_opens_it() {
    let mut world = ended();
    let id = world.session.id;
    let (req_id, sent) = command(&press(&mut world.app, KeyCode::Char('r')));
    assert_eq!(
        sent,
        Command::RestartSession {
            session: id,
            size: PANE,
        }
    );
    world.app.on_frame(ServerFrame::Reply {
        req_id,
        outcome: Ok(Reply::SessionStarted { id }),
    });
    assert_eq!(world.app.mode, Mode::Terminal(id));
}

#[test]
fn a_session_reopened_elsewhere_is_attached_again_if_its_stream_had_ended() {
    let mut world = ended();
    let id = world.session.id;
    // Selected, attached, and the daemon held no terminal for it.
    let session = world.session.clone();
    upsert(&mut world, &session);
    world.app.on_frame(ServerFrame::Detached {
        session: id,
        reason: muster_core::protocol::DetachReason::NotRunning,
    });
    // Another client restarted it.
    let mut reopened = world.session.clone();
    reopened.ended_at = None;
    reopened.status = SessionStatus::Fresh;
    let actions = upsert(&mut world, &reopened);
    assert!(
        actions.iter().any(|action| matches!(
            action,
            Action::Send(ClientFrame::Attach { session, .. }) if *session == id
        )),
        "{actions:?}"
    );

    // The new stream arrives, and from then on follows the session: its
    // next end and reopening need no new attach.
    world.app.on_frame(ServerFrame::Replay {
        session: id,
        seq: 0,
        data: b"$ ".to_vec(),
        reset: true,
    });
    let mut again = reopened.clone();
    again.ended_at = Some(Timestamp::from_millis(20));
    upsert(&mut world, &again);
    let actions = upsert(&mut world, &reopened);
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Send(ClientFrame::Attach { .. }))),
        "{actions:?}"
    );
}

#[test]
fn r_on_a_running_session_or_no_session_explains() {
    let mut world = world();
    world.app.selected = Some(Row::Session(world.session.id));
    assert!(no_command(&press(&mut world.app, KeyCode::Char('r'))));
    let note = world.app.note.clone().expect("a note");
    assert!(
        note.error && note.text.contains("still running"),
        "{note:?}"
    );

    world.app.selected = Some(Row::Worktree(world.worktree.id));
    assert!(no_command(&press(&mut world.app, KeyCode::Char('r'))));
    assert!(world.app.note.as_ref().is_some_and(|note| note.error));
}

#[test]
fn x_removes_an_ended_session_at_once() {
    let mut world = ended();
    let (_, sent) = command(&press(&mut world.app, KeyCode::Char('x')));
    assert_eq!(
        sent,
        Command::RemoveSession {
            session: world.session.id
        }
    );
    assert_eq!(world.app.mode, Mode::Browse, "no question asked");
}

#[test]
fn x_closes_a_running_session_after_asking_once() {
    let mut world = world();
    let id = world.session.id;
    world.app.selected = Some(Row::Session(id));
    assert!(no_command(&press(&mut world.app, KeyCode::Char('x'))));
    assert!(render(&world.app).contains("Close Shell? It stops and leaves the list."));
    let (req_id, sent) = command(&press(&mut world.app, KeyCode::Char('y')));
    assert_eq!(sent, Command::StopSession { session: id });
    world.app.on_frame(ServerFrame::Reply {
        req_id,
        outcome: Ok(Reply::Done),
    });

    // Once it has ended, it is taken off the list without asking again.
    let mut ended = world.session.clone();
    ended.status = SessionStatus::Exited { success: true };
    ended.ended_at = Some(Timestamp::from_millis(9));
    let actions = world
        .app
        .on_frame(ServerFrame::Delta(Delta::SessionUpserted(ended.clone())));
    assert_eq!(command(&actions).1, Command::RemoveSession { session: id });
    // Only once.
    let again = world
        .app
        .on_frame(ServerFrame::Delta(Delta::SessionUpserted(ended)));
    assert!(no_command(&again));
}

#[test]
fn a_session_that_cannot_be_stopped_is_not_closed() {
    let mut world = world();
    let id = world.session.id;
    world.app.selected = Some(Row::Session(id));
    press(&mut world.app, KeyCode::Char('x'));
    let (req_id, _) = command(&press(&mut world.app, KeyCode::Char('y')));
    world.app.on_frame(ServerFrame::Reply {
        req_id,
        outcome: Err(ErrorBody::new(ErrorCode::NotFound, "no such session")),
    });
    assert!(world.app.note.as_ref().is_some_and(|note| note.error));
    // Ending later on its own, it stays listed.
    let mut ended = world.session.clone();
    ended.ended_at = Some(Timestamp::from_millis(9));
    let actions = world
        .app
        .on_frame(ServerFrame::Delta(Delta::SessionUpserted(ended)));
    assert!(no_command(&actions));
}

#[test]
fn x_on_a_worktree_asks() {
    let mut world = world();
    world.app.selected = Some(Row::Worktree(world.worktree.id));
    press(&mut world.app, KeyCode::Char('x'));
    assert!(matches!(world.app.mode, Mode::Dialog(_)));
    assert!(render(&world.app).contains("Remove the worktree for work?"));
}

#[test]
fn a_removed_session_leaves_the_list_and_the_selection_stays_nearby() {
    let mut world = ended();
    let mut second = crate::model::tests::session(world.worktree.id, "Second", 4);
    second.harness = "claude".to_owned();
    upsert(&mut world, &second);
    world.app.selected = Some(Row::Session(world.session.id));
    world.app.on_frame(ServerFrame::Delta(Delta::SessionStatus {
        session: world.session.id,
        status: world.session.status,
        unseen: false,
        since: muster_core::Timestamp::from_millis(10),
    }));

    world
        .app
        .on_frame(ServerFrame::Delta(Delta::SessionRemoved {
            id: world.session.id,
        }));
    assert!(world.app.model.session(world.session.id).is_none());
    assert_eq!(
        world.app.selected,
        Some(Row::Session(second.id)),
        "the line that took its place"
    );
    assert!(!world.app.screens.contains_key(&world.session.id));

    // The last line going away selects the one before it.
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::SessionRemoved { id: second.id }));
    assert_eq!(world.app.selected, Some(Row::Worktree(world.worktree.id)));
}

#[test]
fn a_new_connection_starts_every_screen_over() {
    let mut world = world();
    let id = world.session.id;
    world.app.selected = Some(Row::Session(id));
    world.app.on_frame(ServerFrame::Replay {
        session: id,
        seq: 0,
        data: b"old daemon".to_vec(),
        reset: true,
    });
    assert!(world.app.screens.contains_key(&id));
    world.app.on_disconnected("gone".to_owned());
    world.app.on_connected("0.1.0".to_owned());
    assert!(world.app.screens.is_empty());
}

#[test]
fn hints_follow_the_selection() {
    let mut world = ended();
    let screen = render(&world.app);
    assert!(screen.contains("r restart"), "{screen}");
    assert!(screen.contains("x remove"), "{screen}");
    assert!(!screen.contains("s stop"), "{screen}");

    world.app.selected = Some(Row::Project(world.project.id));
    let screen = render(&world.app);
    assert!(screen.contains("⏎ worktree"), "{screen}");
    assert!(!screen.contains("r restart"), "{screen}");
}

#[test]
fn why_the_daemon_cannot_be_reached_is_shown_in_full() {
    let mut world = world();
    let reason = "the running daemon (muster 0.1.0, protocol 1..=1) is from another build than \
                  this muster (protocol 2..=2); stop it with `muster daemon stop` so this \
                  build's daemon can take over";
    world.app.on_disconnected(reason.to_owned());
    assert!(matches!(world.app.link, Link::Lost(_)));
    let screen = render(&world.app).replace('\n', "");
    assert!(screen.contains("daemon unreachable; retrying"), "{screen}");
    assert!(screen.contains("muster daemon stop"), "{screen}");
}

#[test]
fn a_session_whose_output_is_gone_says_what_can_be_done() {
    let mut world = ended();
    world.app.on_frame(ServerFrame::Detached {
        session: world.session.id,
        reason: muster_core::protocol::DetachReason::NotRunning,
    });
    let screen = render(&world.app).replace('\n', "");
    assert!(screen.contains("Press r to start it again"), "{screen}");
}

#[test]
fn nothing_is_said_about_projects_before_the_daemon_answers() {
    let app = super::App::new(std::path::PathBuf::from("/src"), SIZE);
    let screen = render(&app);
    assert!(screen.contains("Waiting for the daemon"), "{screen}");
    assert!(!screen.contains("No projects yet"), "{screen}");
}

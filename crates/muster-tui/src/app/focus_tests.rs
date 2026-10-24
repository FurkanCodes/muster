//! Where the person is looking: focus, watching, and notifications.

use crossterm::event::KeyCode;
use muster_core::protocol::{ClientFrame, Delta, ServerFrame, Snapshot};
use muster_core::{SessionStatus, WaitReason};

use super::Action;
use super::tests::{World, press, world};
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

#[test]
fn looking_away_and_back_switches_watching_the_attached_session() {
    let mut world = world();
    let id = world.session.id;
    world.app.selected = Some(Row::Worktree(world.worktree.id));
    press(&mut world.app, KeyCode::Down);
    assert_eq!(world.app.focused_session(), Some(id));
    assert_eq!(
        world.app.on_focus(false),
        [
            Action::Send(ClientFrame::Watch {
                session: id,
                watching: false
            }),
            Action::Send(ClientFrame::Looking { looking: false }),
            // Shown, it is in focus as the window is.
            Action::Send(ClientFrame::Keys { session: None })
        ]
    );
    assert_eq!(
        world.app.on_focus(true),
        [
            Action::Send(ClientFrame::Watch {
                session: id,
                watching: true
            }),
            Action::Send(ClientFrame::Looking { looking: true }),
            // Shown, it is in focus as the window is.
            Action::Send(ClientFrame::Keys { session: Some(id) })
        ]
    );
}

#[test]
fn attaching_while_looking_away_says_nobody_watches() {
    let mut world = world();
    world.app.on_focus(false);
    world.app.selected = Some(Row::Worktree(world.worktree.id));
    let actions = press(&mut world.app, KeyCode::Down);
    let session = world.session.id;
    assert!(
        actions.contains(&Action::Send(ClientFrame::Attach {
            session,
            from_seq: None,
            watching: false,
        })),
        "{actions:?}"
    );
}

#[test]
fn news_is_announced_only_while_the_person_looks_elsewhere() {
    let mut world = world();
    let waiting = SessionStatus::NeedsYou {
        reason: WaitReason::Permission,
    };
    // Focused, or never told: nothing.
    assert!(notes(&status(&mut world, waiting, true)).is_empty());
    status(&mut world, SessionStatus::Running, false);
    world.app.on_focus(true);
    assert!(notes(&status(&mut world, waiting, true)).is_empty());
    status(&mut world, SessionStatus::Running, false);

    world.app.on_focus(false);
    assert_eq!(
        notes(&status(&mut world, waiting, true)),
        ["Shell in app (work) needs your permission"]
    );
    // Still waiting: no second notice.
    assert!(notes(&status(&mut world, waiting, true)).is_empty());
    status(&mut world, SessionStatus::Running, false);
    // A finish is told once it has stayed one a moment.
    assert!(notes(&status(&mut world, SessionStatus::Finished, true)).is_empty());
    world.app.on_step(super::alarm::HOLD_STEPS);
    assert_eq!(
        notes(&world.app.due_news()),
        ["Shell in app (work) finished"]
    );
    status(&mut world, SessionStatus::Running, false);
    // A finish someone saw is no news.
    assert!(notes(&status(&mut world, SessionStatus::Finished, false)).is_empty());
}

#[test]
fn a_snapshot_announces_nothing_and_notifications_can_be_turned_off() {
    let mut world = world();
    world.app.on_focus(false);
    let mut session = world.session.clone();
    session.status = SessionStatus::Finished;
    session.unseen = true;
    let actions = world.app.on_frame(ServerFrame::Snapshot(Snapshot {
        projects: vec![world.project.clone()],
        workspaces: vec![world.worktree.clone()],
        sessions: vec![session],
        ..Snapshot::default()
    }));
    assert!(notes(&actions).is_empty());

    status(&mut world, SessionStatus::Running, false);
    world.app.alarms.notify = false;
    assert!(notes(&status(&mut world, SessionStatus::Finished, true)).is_empty());
}

#[test]
fn while_muster_s_helper_notifies_the_terminal_is_told_nothing() {
    let mut world = world();
    world.app.on_focus(false);
    let waiting = SessionStatus::NeedsYou {
        reason: WaitReason::Question,
    };
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::Notifier { ready: true }));
    assert!(notes(&status(&mut world, waiting, true)).is_empty());
    status(&mut world, SessionStatus::Running, false);
    // The helper gone: the terminal tells again.
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::Notifier { ready: false }));
    assert_eq!(notes(&status(&mut world, waiting, true)).len(), 1);
    status(&mut world, SessionStatus::Running, false);
    // Turned off on the Settings screen: nothing at all.
    super::tests::preferences(&mut world.app, |preferences| {
        preferences.notifications = false;
    });
    assert!(notes(&status(&mut world, waiting, true)).is_empty());
}

#[test]
fn notifications_are_turned_on_and_off_on_the_settings_screen() {
    let mut world = world();
    press(&mut world.app, KeyCode::Char('4'));
    world.app.settings.selected = Some(super::SettingRow::Notifications);
    let screen = super::tests::render(&world.app);
    assert!(screen.contains("Notifications"), "{screen}");
    let actions = press(&mut world.app, KeyCode::Enter);
    assert!(
        actions.iter().any(|action| matches!(
            action,
            Action::Send(ClientFrame::Command {
                command: muster_core::protocol::Command::ChangePreference(
                    muster_core::settings::PreferenceChange::Notifications(false)
                ),
                ..
            })
        )),
        "{actions:?}"
    );
}

#[test]
fn on_a_mac_the_helper_starts_for_notifications_with_the_notch_off() {
    let mut world = world();
    world.app.person.notch_here = true;
    world.app.on_connected("0.1.0".to_owned());
    let actions = world.app.on_frame(ServerFrame::Snapshot(Snapshot {
        projects: vec![world.project.clone()],
        preferences: muster_core::settings::Preferences {
            notch: false,
            ..muster_core::settings::Preferences::default()
        },
        ..Snapshot::default()
    }));
    assert!(actions.contains(&Action::StartNotch), "{actions:?}");
}

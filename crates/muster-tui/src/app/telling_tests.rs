//! Which notifications, and their sounds (`PRODUCT.md` §6): a row for
//! questions, finishes and errors under *Notifications*, ⏎ telling each or
//! not, ← → choosing its sound, heard on a Mac as it is chosen; and a kind
//! turned off is not told by the terminal either.

use crossterm::event::KeyCode;
use muster_core::protocol::{Command, Delta, ErrorBody, ErrorCode, Reply, ServerFrame};
use muster_core::settings::{PreferenceChange, TELLING_PROTOCOL};
use muster_core::telling::{Kind, Sound, Tell};
use muster_core::{SessionStatus, WaitReason};

use super::tests::{World, command, preferences, press, render, world};
use super::{Action, SettingRow, View};

/// The world on the Settings screen, `kind`'s row selected.
fn on(kind: Kind) -> World {
    let mut world = world();
    world.app.view = View::Settings;
    world.app.settings_selected = Some(SettingRow::Tell(kind));
    world
}

fn tell(actions: &[Action]) -> (u64, Kind, Tell) {
    let (req_id, sent) = command(actions);
    let Command::ChangePreference(PreferenceChange::Tell { kind, tell }) = sent else {
        panic!("{sent:?}");
    };
    (req_id, kind, tell)
}

fn done(world: &mut World, req_id: u64) {
    world.app.on_frame(ServerFrame::Reply {
        req_id,
        outcome: Ok(Reply::Done),
    });
}

#[test]
fn each_kind_has_a_row_under_notifications_where_the_daemon_keeps_them() {
    let mut world = on(Kind::Finish);
    let screen = render(&world.app);
    for row in ["Questions", "Finishes", "Errors"] {
        let line = screen
            .lines()
            .find(|line| line.contains(row))
            .unwrap_or_else(|| panic!("{row}: {screen}"));
        assert!(line.contains("on · Mac's sound"), "{line}");
    }
    assert!(screen.contains("finished two seconds"), "{screen}");
    assert!(
        screen.contains("[notifications] finishes, finishes_sound"),
        "{screen}"
    );
    assert!(screen.contains("← → sound"), "{screen}");

    world.app.daemon_protocol = TELLING_PROTOCOL - 1;
    assert!(
        !world
            .app
            .setting_rows()
            .contains(&SettingRow::Tell(Kind::Finish))
    );
}

#[test]
fn enter_tells_a_kind_or_not_and_waits_for_the_answer() {
    let mut world = on(Kind::Finish);
    let (req_id, kind, told) = tell(&press(&mut world.app, KeyCode::Enter));
    assert_eq!(
        (kind, told.clone()),
        (
            Kind::Finish,
            Tell {
                on: false,
                sound: Sound::Default
            }
        )
    );
    assert!(press(&mut world.app, KeyCode::Enter).is_empty(), "waits");
    preferences(&mut world.app, |preferences| {
        preferences.telling = preferences.telling.clone().with(Kind::Finish, told);
    });
    done(&mut world, req_id);
    let screen = render(&world.app);
    assert!(screen.contains("Finishes are not told."), "{screen}");
    let row = screen
        .lines()
        .find(|line| line.contains("Finishes"))
        .unwrap();
    assert!(row.contains("off"), "{row}");

    // Refused: said, and it can be tried again.
    let (req_id, ..) = tell(&press(&mut world.app, KeyCode::Enter));
    world.app.on_frame(ServerFrame::Reply {
        req_id,
        outcome: Err(ErrorBody::new(
            ErrorCode::BadRequest,
            "settings.toml: cannot be written",
        )),
    });
    assert!(render(&world.app).contains("cannot be written"));
    assert!(!press(&mut world.app, KeyCode::Enter).is_empty());
}

#[test]
fn the_arrows_choose_a_sound_heard_on_a_mac_as_it_is_chosen() {
    let mut world = on(Kind::Question);
    world.app.notch_here = true;
    // From the Mac's own: none, then the first of the Mac's sounds.
    let actions = press(&mut world.app, KeyCode::Right);
    let (req_id, _, told) = tell(&actions);
    assert_eq!(told.sound, Sound::Silent);
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Hear(_)))
    );
    preferences(&mut world.app, |preferences| {
        preferences.telling = preferences.telling.clone().with(Kind::Question, told);
    });
    done(&mut world, req_id);
    let actions = press(&mut world.app, KeyCode::Right);
    let (_, kind, told) = tell(&actions);
    assert_eq!(kind, Kind::Question);
    assert_eq!(
        told,
        Tell {
            on: true,
            sound: Sound::Named("Basso".to_owned())
        }
    );
    assert!(actions.contains(&Action::Hear(Sound::Named("Basso".to_owned()))));

    // Not on a Mac: chosen, not heard.
    let mut world = on(Kind::Error);
    let actions = press(&mut world.app, KeyCode::Left);
    assert_eq!(tell(&actions).2.sound, Sound::Named("Tink".to_owned()));
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Hear(_)))
    );
}

#[test]
fn a_kind_turned_off_is_not_told_by_the_terminal_either() {
    let mut world = world();
    preferences(&mut world.app, |preferences| {
        preferences.telling = preferences.telling.clone().with(
            Kind::Question,
            Tell {
                on: false,
                sound: Sound::Default,
            },
        );
    });
    world.app.on_focus(false);
    let session = world.session.id;
    let status = |world: &mut World, status: SessionStatus, unseen: bool| {
        world.app.on_frame(ServerFrame::Delta(Delta::SessionStatus {
            session,
            status,
            unseen,
            since: muster_core::Timestamp::from_millis(10),
        }))
    };
    let notified = |actions: &[Action]| {
        actions
            .iter()
            .any(|action| matches!(action, Action::Notify(_)))
    };
    let asking = SessionStatus::NeedsYou {
        reason: WaitReason::Permission,
    };
    assert!(!notified(&status(&mut world, asking, true)), "off");
    status(&mut world, SessionStatus::Running, false);
    let failed = SessionStatus::Exited { success: false };
    assert!(notified(&status(&mut world, failed, true)), "errors are on");
}

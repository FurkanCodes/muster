//! The notch (`ARC-22`): telling the daemon whether the person looks at
//! this window, showing a session picked in the notch, starting the notch,
//! and turning it on and off on the Settings screen.

use crossterm::event::KeyCode;
use muster_core::protocol::{
    ClientFrame, Command, Delta, ErrorBody, ErrorCode, Reply, ServerFrame, Snapshot,
};
use muster_core::settings::{PreferenceChange, Preferences};

use super::tests::{World, command, press, render, world};
use super::{Action, App, Mode, SettingRow, View};
use crate::model::Row;
use muster_core::telling::Kind;

fn looking(looking: bool) -> Action {
    Action::Send(ClientFrame::Looking { looking })
}

/// The world's app, on a Mac, connected anew with Muster's own settings.
fn mac(notch: bool) -> World {
    let mut world = world();
    world.app.notch_here = true;
    world.app.on_connected("0.1.0".to_owned());
    world.app.on_frame(ServerFrame::Snapshot(Snapshot {
        projects: vec![world.project.clone()],
        workspaces: vec![world.worktree.clone()],
        sessions: vec![world.session.clone()],
        preferences: Preferences {
            notch,
            notifications: false,
            file: "/home/me/.config/muster/settings.toml".into(),
            ..Preferences::default()
        },
        ..Snapshot::default()
    }));
    world
}

fn preferences(notch: bool) -> ServerFrame {
    ServerFrame::Delta(Delta::Preferences(Preferences {
        notch,
        notifications: false,
        file: "/home/me/.config/muster/settings.toml".into(),
        ..Preferences::default()
    }))
}

#[test]
fn the_daemon_is_told_whether_the_person_looks_at_this_window() {
    let mut app = App::new("/src".into(), super::tests::SIZE);
    // A terminal that never says counts as looked at.
    let connected = app.on_connected("0.1.0".to_owned());
    assert_eq!(
        connected,
        [Action::Send(ClientFrame::Subscribe), looking(true)]
    );
    assert!(app.on_focus(false).contains(&looking(false)));
    assert!(app.on_focus(true).contains(&looking(true)));
    // Reconnected while looking away: said again.
    app.on_focus(false);
    app.on_disconnected("gone".to_owned());
    assert!(
        !app.on_focus(false).contains(&looking(false)),
        "not connected"
    );
    assert!(
        app.on_connected("0.1.0".to_owned())
            .contains(&looking(false))
    );
}

#[test]
fn a_session_picked_in_the_notch_opens_here_with_the_keys() {
    let mut world = world();
    let session = world.session.id;
    world.app.view = View::Tickets;
    world.app.welcome = true;
    let actions = world.app.on_frame(ServerFrame::Reveal { session });
    assert!(actions.contains(&Action::Activate), "{actions:?}");
    assert!(
        actions.contains(&Action::Send(ClientFrame::Attach {
            session,
            from_seq: None,
            watching: true,
        })),
        "{actions:?}"
    );
    assert_eq!(world.app.view, View::Sessions);
    assert_eq!(world.app.selected, Some(Row::Session(session)));
    assert_eq!(world.app.mode, Mode::Terminal(session));
    assert!(!world.app.welcome);

    // One the UI does not know is left alone.
    let unknown = muster_core::SessionId::new();
    assert!(
        world
            .app
            .on_frame(ServerFrame::Reveal { session: unknown })
            .is_empty()
    );
}

#[test]
fn the_notch_is_started_once_wanted_and_again_once_turned_back_on() {
    // Not on a Mac: never.
    let mut world = world();
    assert!(
        !world
            .app
            .on_frame(preferences(true))
            .contains(&Action::StartNotch)
    );

    let mut world = mac(true);
    // Started with the snapshot, in `mac`; not again.
    assert!(
        !world
            .app
            .on_frame(preferences(true))
            .contains(&Action::StartNotch)
    );
    // Turned off, it stops itself; turned on, it is started again.
    assert!(
        !world
            .app
            .on_frame(preferences(false))
            .contains(&Action::StartNotch)
    );
    assert!(
        world
            .app
            .on_frame(preferences(true))
            .contains(&Action::StartNotch)
    );
    // A new daemon may come without it.
    world.app.on_connected("0.1.0".to_owned());
    let snapshot = world.app.on_frame(ServerFrame::Snapshot(Snapshot {
        preferences: Preferences::default(),
        ..Snapshot::default()
    }));
    assert!(snapshot.contains(&Action::StartNotch), "{snapshot:?}");
}

#[test]
fn the_snapshot_on_a_mac_starts_the_notch() {
    let mut world = world();
    world.app.notch_here = true;
    world.app.on_connected("0.1.0".to_owned());
    let actions = world.app.on_frame(ServerFrame::Snapshot(Snapshot {
        projects: vec![world.project.clone()],
        ..Snapshot::default()
    }));
    assert!(actions.contains(&Action::StartNotch), "{actions:?}");

    // Off in Muster's own settings: no.
    let mut world = mac(false);
    world.app.on_connected("0.1.0".to_owned());
    let off = Snapshot {
        preferences: Preferences {
            notch: false,
            notifications: false,
            file: "/home/me/.config/muster/settings.toml".into(),
            ..Preferences::default()
        },
        ..Snapshot::default()
    };
    let actions = world.app.on_frame(ServerFrame::Snapshot(off));
    assert!(!actions.contains(&Action::StartNotch), "{actions:?}");
}

#[test]
fn the_notch_is_turned_on_and_off_on_the_settings_screen() {
    let mut world = mac(true);
    world.app.view = View::Settings;
    world.app.settings.selected = Some(SettingRow::Notch);
    let screen = render(&world.app);
    assert!(screen.contains("Muster"), "{screen}");
    assert!(screen.contains("Agents in the notch"), "{screen}");
    assert!(screen.contains("settings.toml"), "{screen}");

    let actions = press(&mut world.app, KeyCode::Enter);
    let (req_id, sent) = command(&actions);
    assert_eq!(
        sent,
        Command::ChangePreference(PreferenceChange::Notch(false))
    );
    // A second press waits for the answer.
    assert!(press(&mut world.app, KeyCode::Char(' ')).is_empty());

    // The answer may come before the change to the state.
    world.app.on_frame(ServerFrame::Reply {
        req_id,
        outcome: Ok(Reply::Done),
    });
    assert!(render(&world.app).contains("Muster leaves the notch alone."));
    world.app.on_frame(preferences(false));
    let (req_id, sent) = command(&press(&mut world.app, KeyCode::Enter));
    assert_eq!(
        sent,
        Command::ChangePreference(PreferenceChange::Notch(true))
    );
    world.app.on_frame(ServerFrame::Reply {
        req_id,
        outcome: Err(ErrorBody::new(
            ErrorCode::BadRequest,
            "settings.toml: unknown field `shwo`",
        )),
    });
    assert!(render(&world.app).contains("unknown field"));
    // Refused: it can be tried again.
    assert!(!press(&mut world.app, KeyCode::Enter).is_empty());
}

#[test]
fn a_change_whose_answer_the_lost_connection_took_can_be_asked_again() {
    let mut world = mac(true);
    world.app.view = View::Settings;
    world.app.settings.selected = Some(SettingRow::Notch);
    command(&press(&mut world.app, KeyCode::Enter));
    assert!(world.app.notch_changing());
    world.app.on_disconnected("the daemon restarted".to_owned());
    assert!(!world.app.notch_changing(), "no answer is coming");
    world.app.on_connected("0.1.0".to_owned());
    world.app.on_frame(preferences(true));
    let (_, sent) = command(&press(&mut world.app, KeyCode::Enter));
    assert_eq!(
        sent,
        Command::ChangePreference(PreferenceChange::Notch(false))
    );
}

#[test]
fn muster_s_own_settings_show_the_notch_only_where_it_can_be_and_the_person_s_agents() {
    let mut world = world();
    world.app.view = View::Settings;
    assert!(!world.app.setting_rows().contains(&SettingRow::Notch));
    assert!(world.app.setting_rows().ends_with(&[
        SettingRow::Muster,
        SettingRow::Notifications,
        SettingRow::Tell(Kind::Question),
        SettingRow::Tell(Kind::Finish),
        SettingRow::Tell(Kind::Error),
        SettingRow::SideBySide,
        SettingRow::Colours,
        SettingRow::SessionLines,
        SettingRow::Panels,
        SettingRow::NamesByAgent,
        SettingRow::NamedBy,
        SettingRow::NamingModel,
        SettingRow::Keys,
        SettingRow::SharedBuilds,
        SettingRow::Delegation,
        SettingRow::Agents,
        SettingRow::Plugins,
        SettingRow::AddPlugin
    ]));
    world.app.notch_here = true;
    assert!(world.app.setting_rows().ends_with(&[
        SettingRow::Muster,
        SettingRow::Notch,
        SettingRow::Notifications,
        SettingRow::Tell(Kind::Question),
        SettingRow::Tell(Kind::Finish),
        SettingRow::Tell(Kind::Error),
        SettingRow::SideBySide,
        SettingRow::Colours,
        SettingRow::SessionLines,
        SettingRow::Panels,
        SettingRow::NamesByAgent,
        SettingRow::NamedBy,
        SettingRow::NamingModel,
        SettingRow::Keys,
        SettingRow::SharedBuilds,
        SettingRow::Delegation,
        SettingRow::Agents,
        SettingRow::Plugins,
        SettingRow::AddPlugin
    ]));
}

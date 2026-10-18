//! A session's terminal as in a real one: the person's colours told to the
//! daemon.

use std::path::PathBuf;

use muster_core::colours::{Rgb, TerminalColours};
use muster_core::protocol::ClientFrame;

use super::tests::SIZE;
use super::{Action, App};

fn dark() -> TerminalColours {
    TerminalColours {
        background: Some(Rgb {
            red: 0x1e1e,
            green: 0x1e1e,
            blue: 0x2e2e,
        }),
        ..TerminalColours::default()
    }
}

#[test]
fn the_person_s_colours_are_told_to_each_daemon_that_knows_them() {
    let mut app = App::new(PathBuf::from("/src"), SIZE);
    app.colours = dark();
    let told = Action::Send(ClientFrame::Colours(dark()));
    assert!(app.on_connected("0.4.0".to_owned()).contains(&told));
    // Again on every connection: a daemon started since knows none.
    app.on_disconnected("gone".to_owned());
    assert!(app.on_connected("0.4.0".to_owned()).contains(&told));
    // Never to a daemon before them, which could not read it (`ARC-09`).
    app.daemon_protocol = muster_core::colours::TERMINAL_PROTOCOL - 1;
    assert!(!app.on_connected("0.3.25".to_owned()).contains(&told));
}

#[test]
fn a_terminal_that_said_no_colours_has_none_told() {
    let mut app = App::new(PathBuf::from("/src"), SIZE);
    let actions = app.on_connected("0.4.0".to_owned());
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Send(ClientFrame::Colours(_)))),
        "{actions:?}"
    );
}

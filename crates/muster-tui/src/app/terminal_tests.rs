//! A session's terminal as in a real one: the person's colours told to the
//! daemon, a program's copy put on their clipboard, and the daemon told
//! which session is in focus, for programs that follow their focus, and a
//! session's size in pixels.

use std::path::PathBuf;

use crossterm::event::KeyCode;
use muster_core::colours::{Rgb, TerminalColours};
use muster_core::protocol::{ClientFrame, ServerFrame, TermSize};

use super::tests::{SIZE, World, press, render, shift_q, world};
use super::{Action, App, View};
use crate::model::Row;

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

/// The world's session open beside the list, its history replayed.
fn opened(history: &[u8]) -> World {
    let mut world = world();
    let id = world.session.id;
    world.app.selected = Some(Row::Session(id));
    press(&mut world.app, KeyCode::Enter);
    world.app.on_frame(ServerFrame::Replay {
        session: id,
        seq: 0,
        data: history.to_vec(),
        reset: true,
        live_from: None,
        screen: None,
    });
    world
}

fn output(world: &mut World, data: &[u8]) -> Vec<Action> {
    let session = world.session.id;
    let seq = world.app.screens[&session].next_seq().unwrap_or(0);
    world.app.on_frame(ServerFrame::Output {
        session,
        seq,
        data: data.to_vec(),
    })
}

fn copies(actions: &[Action]) -> Vec<&str> {
    actions
        .iter()
        .filter_map(|action| match action {
            Action::Copy(text) => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

const NONE: [&str; 0] = [];

#[test]
fn a_program_s_copy_in_its_live_output_reaches_the_clipboard() {
    let mut world = opened(b"$ ");
    // "copy-42\nok", ended with BEL, then "next-8" with ST.
    let actions = output(&mut world, b"\x1b]52;c;Y29weS00Mgpvaw==\x07$ ");
    assert_eq!(copies(&actions), ["copy-42\nok"]);
    assert!(render(&world.app).contains("The session copied 2 lines."));
    let actions = output(&mut world, b"\x1b]52;c;bmV4dC04\x1b\\");
    assert_eq!(copies(&actions), ["next-8"]);
    // Split across two pieces of output, as reads cut it anywhere.
    assert_eq!(copies(&output(&mut world, b"\x1b]52;c;Y29w")), NONE);
    assert_eq!(copies(&output(&mut world, b"eS00Mg==\x07")), ["copy-42"]);
}

#[test]
fn a_copy_in_a_session_s_history_is_never_made_again_on_attaching() {
    // A copy the program made before: in the history replayed.
    let mut world = opened(b"\x1b]52;c;Y29weS00Mgpvaw==\x07$ ");
    let id = world.session.id;
    assert_eq!(copies(&output(&mut world, b"ls\r\n")), NONE);
    // Attached again from where the screen left off, after a reconnect:
    // what the replay brings is history too.
    let actions = world.app.on_frame(ServerFrame::Replay {
        session: id,
        seq: world.app.screens[&id].next_seq().unwrap(),
        data: b"\x1b]52;c;bmV4dC04\x07".to_vec(),
        reset: false,
        live_from: None,
        screen: None,
    });
    assert_eq!(copies(&actions), NONE);
    assert!(!render(&world.app).contains("copied"));
}

#[test]
fn a_copy_the_client_was_sent_late_as_it_fell_behind_reaches_the_clipboard() {
    let mut world = opened(b"$ ");
    let id = world.session.id;
    let next = world.app.screens[&id].next_seq().unwrap();
    // What the program wrote while the person looked on, sent again from
    // the ring as this client fell behind: "next-8".
    let actions = world.app.on_frame(ServerFrame::Replay {
        session: id,
        seq: next,
        data: b"\x1b]52;c;bmV4dC04\x07".to_vec(),
        reset: false,
        live_from: Some(next),
        screen: None,
    });
    assert_eq!(copies(&actions), ["next-8"]);
    assert!(render(&world.app).contains("The session copied a line."));
}

#[test]
fn a_program_never_reads_the_clipboard_nor_copies_what_is_not_text() {
    let mut world = opened(b"$ ");
    // A read, a copy of an escape sequence, and one past the limit.
    let mut actions = output(&mut world, b"\x1b]52;c;?\x07");
    actions.extend(output(&mut world, b"\x1b]52;c;G1s4bQ==\x07"));
    let large = base64_of(&"x".repeat(crate::clipboard::MAX_COPY + 1));
    let past = [b"\x1b]52;c;", large.as_bytes(), b"\x07"].concat();
    actions.extend(output(&mut world, &past));
    assert_eq!(copies(&actions), NONE);
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Send(ClientFrame::Input { .. }))),
        "nothing answers the read: {actions:?}"
    );
}

fn base64_of(text: &str) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(text)
}

fn focus_told(actions: &[Action]) -> Vec<Option<muster_core::SessionId>> {
    actions
        .iter()
        .filter_map(|action| match action {
            Action::Send(ClientFrame::Keys { session }) => Some(*session),
            _ => None,
        })
        .collect()
}

#[test]
fn a_session_on_screen_is_in_focus_so_claude_code_suggests_the_next_prompt() {
    let mut world = world();
    let id = world.session.id;
    world.app.selected = Some(Row::Session(id));
    // Shown beside the list, it is in focus without the keys.
    assert_eq!(focus_told(&world.app.settle()), [Some(id)]);
    // Typing in it, filling the window and coming back change nothing.
    assert_eq!(focus_told(&press(&mut world.app, KeyCode::Enter)), []);
    assert_eq!(focus_told(&press(&mut world.app, KeyCode::Char('l'))), []);
    assert_eq!(focus_told(&world.app.on_key(shift_q())), []);
    assert_eq!(focus_told(&press(&mut world.app, KeyCode::Char('f'))), []);
    assert_eq!(focus_told(&world.app.on_key(shift_q())), []);
    // The window looked away from, and back at.
    assert_eq!(focus_told(&world.app.on_focus(false)), [None]);
    assert_eq!(focus_told(&world.app.on_focus(true)), [Some(id)]);
    // Nothing shown: a project selected, then another screen.
    world.app.selected = Some(Row::Project(world.project.id));
    assert_eq!(focus_told(&world.app.settle()), [None]);
    world.app.selected = Some(Row::Session(id));
    assert_eq!(focus_told(&world.app.settle()), [Some(id)]);
    world.app.view = View::Tickets;
    assert_eq!(focus_told(&world.app.settle()), [None]);
}

#[test]
fn a_narrow_window_s_list_shows_no_session_in_focus_until_one_is_open() {
    let mut world = world();
    let id = world.session.id;
    world.app.selected = Some(Row::Session(id));
    world.app.on_resize(TermSize { cols: 80, rows: 30 });
    assert!(world.app.narrow());
    assert_eq!(focus_told(&world.app.settle()), []);
    assert_eq!(
        focus_told(&press(&mut world.app, KeyCode::Enter)),
        [Some(id)]
    );
    assert_eq!(focus_told(&world.app.on_key(shift_q())), [None]);
}

#[test]
fn a_daemon_connected_again_is_told_the_keys_and_an_older_one_never() {
    let mut world = world();
    let id = world.session.id;
    world.app.selected = Some(Row::Session(id));
    press(&mut world.app, KeyCode::Enter);
    world.app.on_disconnected("gone".to_owned());
    world.app.on_connected("0.4.0".to_owned());
    let snapshot = muster_core::protocol::Snapshot {
        projects: vec![world.project.clone()],
        workspaces: vec![world.worktree.clone()],
        sessions: vec![world.session.clone()],
        ..Default::default()
    };
    let actions = world.app.on_frame(ServerFrame::Snapshot(snapshot.clone()));
    assert_eq!(focus_told(&actions), [Some(id)]);

    // A daemon before it could not read the frame (`ARC-09`).
    world.app.on_disconnected("gone".to_owned());
    world.app.daemon_protocol = muster_core::colours::TERMINAL_PROTOCOL - 1;
    world.app.on_connected("0.3.25".to_owned());
    let mut actions = world.app.on_frame(ServerFrame::Snapshot(snapshot));
    actions.extend(world.app.on_focus(false));
    actions.extend(world.app.on_key(shift_q()));
    assert_eq!(focus_told(&actions), []);
}

#[test]
fn a_session_s_size_in_pixels_is_scaled_from_the_person_s_cells() {
    use muster_core::protocol::PixelSize;
    let mut world = world();
    let id = world.session.id;
    world.app.cell_pixels = Some(PixelSize {
        width: 8,
        height: 16,
    });
    world.app.selected = Some(Row::Session(id));
    let actions = world.app.settle();
    let pixels = actions.iter().find_map(|action| match action {
        Action::Send(ClientFrame::Resize { size, pixels, .. }) => Some((*size, *pixels)),
        _ => None,
    });
    let pane = super::tests::PANE;
    assert_eq!(
        pixels,
        Some((
            pane,
            Some(PixelSize {
                width: pane.cols * 8,
                height: pane.rows * 16
            })
        ))
    );
    // Full screen, at the window's size.
    let actions = press(&mut world.app, KeyCode::Char('f'));
    assert!(
        actions.contains(&Action::Send(ClientFrame::Resize {
            session: id,
            size: SIZE,
            pixels: Some(PixelSize {
                width: SIZE.cols * 8,
                height: SIZE.rows * 16
            }),
        })),
        "{actions:?}"
    );
}

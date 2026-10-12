//! Getting about: the four screens as tabs, ⇥ round them, Ctrl and their
//! numbers straight to one from anywhere, Esc and Backspace back to the
//! main one.

use crossterm::event::KeyCode;
use muster_core::protocol::{ClientFrame, Command};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::Modifier;

use super::tests::{SIZE, World, ctrl, press, render, world};
use super::{Action, Mode, View};
use crate::model::Row;

fn at(world: &mut World, view: View) {
    world.app.view = view;
}

#[test]
fn esc_and_backspace_go_back_to_the_main_screen_and_no_further() {
    for key in [KeyCode::Esc, KeyCode::Backspace] {
        for from in [View::NeedsYou, View::Tickets, View::Settings] {
            let mut world = world();
            at(&mut world, from);
            press(&mut world.app, key);
            assert_eq!(world.app.view, View::Sessions, "{key:?} from {from:?}");
        }
        // On the main screen there is nothing to go back to: it stays, and
        // nothing quits.
        let mut world = world();
        let actions = press(&mut world.app, key);
        assert_eq!(world.app.view, View::Sessions);
        assert!(!actions.contains(&Action::Quit));
    }
    let world = world();
    assert!(!render(&world.app).contains("esc back"));
    let mut world = world;
    at(&mut world, View::NeedsYou);
    assert!(render(&world.app).contains("esc back"));
}

#[test]
fn ctrl_and_a_number_jumps_to_each_screen_from_any_of_them() {
    let mut world = world();
    let actions = world.app.on_key(ctrl('3'));
    assert_eq!(world.app.view, View::Tickets);
    // The tickets are fetched as the screen shows them.
    assert!(actions.iter().any(|action| matches!(
        action,
        Action::Send(ClientFrame::Command {
            command: Command::ListIssues { .. },
            ..
        })
    )));
    // From the Tickets screen too, whose own keys take the rest.
    world.app.on_key(ctrl('2'));
    assert_eq!(world.app.view, View::NeedsYou);
    world.app.on_key(ctrl('1'));
    assert_eq!(world.app.view, View::Sessions);
    // The plain numbers too, on the list, for terminals that cannot tell
    // Ctrl and a number from the number.
    press(&mut world.app, KeyCode::Char('2'));
    assert_eq!(world.app.view, View::NeedsYou);
    press(&mut world.app, KeyCode::Char('1'));
    assert_eq!(world.app.view, View::Sessions);
    // Settings is 4 on the list, with Ctrl or without.
    press(&mut world.app, KeyCode::Char('4'));
    assert_eq!(world.app.view, View::Settings);
    world.app.on_key(ctrl('1'));
    world.app.on_key(ctrl('4'));
    assert_eq!(world.app.view, View::Settings);
}

#[test]
fn in_a_session_s_terminal_ctrl_4_leaves_the_keys_with_it() {
    // Without the kitty protocol Ctrl+4 and Ctrl+\ are one byte, which a
    // session's program may want: Settings is not taken from there.
    let mut world = world();
    let id = world.session.id;
    world.app.selected = Some(Row::Session(id));
    press(&mut world.app, KeyCode::Enter);
    world.app.on_key(ctrl('4'));
    assert_eq!(world.app.mode, Mode::Terminal(id));
    assert_eq!(world.app.view, View::Sessions);
}

#[test]
fn from_a_session_s_terminal_ctrl_and_a_number_gives_the_keys_back_there() {
    let mut world = world();
    let id = world.session.id;
    world.app.selected = Some(Row::Session(id));
    press(&mut world.app, KeyCode::Enter);
    assert_eq!(world.app.mode, Mode::Terminal(id));
    // A plain number is the program's.
    assert_eq!(
        press(&mut world.app, KeyCode::Char('2')),
        [Action::Send(ClientFrame::Input {
            session: id,
            data: b"2".to_vec()
        })]
    );
    world.app.on_key(ctrl('2'));
    assert_eq!(world.app.mode, Mode::Browse);
    assert_eq!(world.app.view, View::NeedsYou);

    // Ctrl-1 is the way back to the list beside it, as Shift-Q is; from
    // full screen too.
    world.app.view = View::Sessions;
    press(&mut world.app, KeyCode::Char('f'));
    assert!(world.app.full_screen);
    world.app.on_key(ctrl('1'));
    assert_eq!(
        (world.app.mode.clone(), world.app.view),
        (Mode::Browse, View::Sessions)
    );
    assert!(!world.app.full_screen);
}

#[test]
fn tab_goes_round_the_tabs_and_in_a_session_reaches_the_program() {
    let mut world = world();
    for view in [
        View::NeedsYou,
        View::Tickets,
        View::Settings,
        View::Sessions,
    ] {
        press(&mut world.app, KeyCode::Tab);
        assert_eq!(world.app.view, view);
    }
    press(&mut world.app, KeyCode::BackTab);
    assert_eq!(world.app.view, View::Settings);
    press(&mut world.app, KeyCode::BackTab);
    assert_eq!(world.app.view, View::Tickets);

    // ⏎ goes into a session, where ⇥ is the program's: completion.
    world.app.view = View::Sessions;
    let id = world.session.id;
    world.app.selected = Some(Row::Session(id));
    press(&mut world.app, KeyCode::Enter);
    assert_eq!(
        press(&mut world.app, KeyCode::Tab),
        [Action::Send(ClientFrame::Input {
            session: id,
            data: b"\t".to_vec()
        })]
    );
    assert_eq!(world.app.view, View::Sessions);
}

#[test]
fn esc_in_a_dialog_closes_it_and_the_screen_stays() {
    let mut world = world();
    at(&mut world, View::NeedsYou);
    world.app.needs_selected = Some(world.session.id);
    press(&mut world.app, KeyCode::Char('s'));
    assert!(matches!(world.app.mode, Mode::Dialog(_)));
    press(&mut world.app, KeyCode::Esc);
    assert_eq!(world.app.mode, Mode::Browse);
    assert_eq!(world.app.view, View::NeedsYou);
}

#[test]
fn in_a_session_s_terminal_esc_and_backspace_reach_the_program() {
    let mut world = world();
    world.app.selected = Some(Row::Session(world.session.id));
    press(&mut world.app, KeyCode::Enter);
    assert_eq!(world.app.mode, Mode::Terminal(world.session.id));
    for (key, byte) in [(KeyCode::Esc, 0x1b), (KeyCode::Backspace, 0x7f)] {
        let actions = press(&mut world.app, key);
        assert_eq!(
            actions,
            [Action::Send(ClientFrame::Input {
                session: world.session.id,
                data: vec![byte],
            })]
        );
        assert_eq!(world.app.mode, Mode::Terminal(world.session.id));
    }
}

#[test]
fn the_top_bar_shows_the_screens_as_tabs_with_the_one_shown_marked() {
    let mut world = world();
    for (view, name) in [
        (View::Sessions, "Sessions"),
        (View::NeedsYou, "Needs you"),
        (View::Tickets, "Tickets"),
        (View::Settings, "Settings"),
    ] {
        at(&mut world, view);
        let mut terminal = Terminal::new(TestBackend::new(SIZE.cols, SIZE.rows + 1)).unwrap();
        terminal
            .draw(|frame| crate::view::draw(frame, &world.app))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let top: String = (0..buffer.area.width)
            .map(|x| buffer[(x, 0)].symbol())
            .collect();
        assert!(
            top.contains(" Sessions │ Needs you │ Tickets │ Settings  ⇥"),
            "{top}"
        );
        // Each tab's name is marked only while its screen is shown.
        for other in ["Sessions", "Needs you", "Tickets", "Settings"] {
            let at = top.find(other).unwrap();
            let column = u16::try_from(top[..at].chars().count()).unwrap();
            let marked = buffer[(column, 0)].modifier.contains(Modifier::REVERSED);
            assert_eq!(marked, other == name, "{other} while {name} is shown");
        }
    }
}

//! A dialog by the mouse (`ARC-32`): a key its hint names is pressed by a
//! click; an agent offered is chosen by one; a click outside closes it; a
//! field, a row of the keys' dialog, a choice is reached by one; and the
//! wheel moves through what it offers.

use crossterm::event::{KeyCode, MouseButton, MouseEventKind};
use muster_core::keys::Action as Does;
use muster_core::protocol::{Command, TermSize};
use ratatui::layout::{Position, Rect};

use super::pointer_tests::mouse;
use super::tests::{World, command, press, world};
use super::{Confirmed, Dialog, HarnessChoice, Mode};
use crate::model::Row;
use crate::view::dialogs::DialogPress;

const WIDE: TermSize = TermSize {
    cols: 160,
    rows: 40,
};

/// Where `wanted` is in the dialog open, its first cell.
fn press_of(world: &World, wanted: &DialogPress) -> Position {
    let Mode::Dialog(dialog) = &world.app.mode else {
        panic!("no dialog: {:?}", world.app.mode);
    };
    let size = world.app.terminal_size;
    let window = Rect::new(0, 0, size.cols, size.rows + 1);
    let (_, presses) = crate::view::dialogs::dialog_presses(&world.app, dialog, window);
    presses
        .iter()
        .find(|(_, press)| press == wanted)
        .map_or_else(
            || panic!("no {wanted:?} in {presses:?}"),
            |(cells, _)| Position::new(cells.x, cells.y),
        )
}

fn click(world: &mut World, at: Position) -> Vec<super::Action> {
    world
        .app
        .on_mouse(mouse(MouseEventKind::Down(MouseButton::Left), at.x, at.y))
}

fn key(code: KeyCode) -> DialogPress {
    DialogPress::Key(crossterm::event::KeyEvent::from(code))
}

#[test]
fn a_question_s_answers_are_clicked_and_a_click_outside_closes_it() {
    let mut world = world();
    world.app.on_resize(WIDE);
    world.app.mode = Mode::Dialog(Dialog::Confirm {
        question: "Close Fix the login? It stops and leaves the list.".to_owned(),
        then: Confirmed::CloseSession(world.session.id),
    });
    let yes = press_of(
        &world,
        &DialogPress::Key(crate::keys::labelled("y").unwrap()),
    );
    let actions = click(&mut world, yes);
    assert!(
        matches!(command(&actions).1, Command::StopSession { .. }),
        "{actions:?}"
    );
    // Asked again, and a click outside: no.
    world.app.mode = Mode::Dialog(Dialog::Confirm {
        question: "Close it?".to_owned(),
        then: Confirmed::CloseSession(world.session.id),
    });
    click(&mut world, Position::new(1, 1));
    assert_eq!(world.app.mode, super::Mode::Browse);
}

#[test]
fn an_agent_offered_is_chosen_by_a_click_and_start_by_another() {
    let mut world = world();
    world.app.on_resize(WIDE);
    world.app.selected = Some(Row::Worktree(world.worktree.id));
    world.app.settle();
    press(&mut world.app, KeyCode::Char('n'));
    let codex = HarnessChoice::all(&[])
        .into_iter()
        .find(|choice| choice.id() == "codex")
        .unwrap();
    let at = press_of(&world, &DialogPress::Harness(codex.clone()));
    click(&mut world, at);
    let Mode::Dialog(Dialog::NewSession { harness, .. }) = &world.app.mode else {
        panic!("{:?}", world.app.mode);
    };
    assert_eq!(*harness, codex);
    let start = press_of(&world, &key(KeyCode::Enter));
    let actions = click(&mut world, start);
    let Command::StartSession { harness, .. } = command(&actions).1 else {
        panic!("{actions:?}");
    };
    assert_eq!(harness, "codex");
}

#[test]
fn a_row_of_the_keys_dialog_is_chosen_by_a_click_and_set_by_a_double_click() {
    let mut world = world();
    world.app.on_resize(WIDE);
    press(&mut world.app, KeyCode::Char('4'));
    world.app.settings.selected = Some(super::SettingRow::Keys);
    press(&mut world.app, KeyCode::Enter);
    let rows = world.app.rebind_rows();
    // A row further down, and one back up.
    for wanted in [Does::Help, Does::Quit] {
        let at = rows
            .iter()
            .position(|row| *row == super::RebindRow::Key(super::Rebound::Does(wanted)))
            .unwrap();
        let cells = press_of(&world, &DialogPress::Row(at));
        click(&mut world, cells);
        let Mode::Dialog(Dialog::Keys(rebinding)) = &world.app.mode else {
            panic!("{:?}", world.app.mode);
        };
        assert_eq!(
            world.app.rebinding_chosen(rebinding),
            super::Rebound::Does(wanted)
        );
    }
    // Twice, quickly: it waits for the key to set.
    let at = rows
        .iter()
        .position(|row| *row == super::RebindRow::Key(super::Rebound::Does(Does::Quit)))
        .unwrap();
    let cells = press_of(&world, &DialogPress::Row(at));
    for ms in [1_000, 1_100] {
        world.app.on_mouse_at(
            mouse(MouseEventKind::Down(MouseButton::Left), cells.x, cells.y),
            ms,
        );
    }
    let Mode::Dialog(Dialog::Keys(rebinding)) = &world.app.mode else {
        panic!("{:?}", world.app.mode);
    };
    assert!(rebinding.waiting);
}

#[test]
fn the_wheel_moves_through_what_a_dialog_offers() {
    let mut world = world();
    world.app.on_resize(WIDE);
    press(&mut world.app, KeyCode::Char('4'));
    world.app.settings.selected = Some(super::SettingRow::Keys);
    press(&mut world.app, KeyCode::Enter);
    let chosen = |world: &World| match &world.app.mode {
        Mode::Dialog(Dialog::Keys(rebinding)) => rebinding.chosen,
        mode => panic!("{mode:?}"),
    };
    let before = chosen(&world);
    world
        .app
        .on_mouse(mouse(MouseEventKind::ScrollDown, 80, 20));
    assert_eq!(chosen(&world), before + 1);
}

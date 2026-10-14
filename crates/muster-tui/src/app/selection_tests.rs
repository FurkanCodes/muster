//! Selecting a session's text with the mouse and copying it, beside the
//! list and full screen.

use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use muster_core::protocol::ServerFrame;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::Modifier;

use super::pointer_tests::{input, mouse, mouse_mode, open_with_output, set_modes};
use super::tests::{SIZE, World, press, render};
use super::{Action, Mode};
use crate::model::Row;

/// Beside the list, the session's screen starts after the 34-column list
/// and the pane's border, below the top bar and the border.
const PANE_X: u16 = 35;
const PANE_Y: u16 = 2;

fn left(kind: fn(MouseButton) -> MouseEventKind, column: u16, row: u16) -> MouseEvent {
    mouse(kind(MouseButton::Left), column, row)
}

/// A press at `from`, a drag to `to` and the release there, as (column,
/// row) of the window; what the last event asked.
fn drag(world: &mut World, from: (u16, u16), to: (u16, u16)) -> Vec<Action> {
    let mut actions = world
        .app
        .on_mouse(left(MouseEventKind::Down, from.0, from.1));
    actions.extend(world.app.on_mouse(left(MouseEventKind::Drag, to.0, to.1)));
    actions.extend(world.app.on_mouse(left(MouseEventKind::Up, to.0, to.1)));
    actions
}

fn copied(actions: &[Action]) -> Option<&str> {
    actions.iter().find_map(|action| match action {
        Action::Copy(text) => Some(text.as_str()),
        _ => None,
    })
}

fn drawn(world: &World) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(SIZE.cols, SIZE.rows + 1)).unwrap();
    terminal
        .draw(|frame| crate::view::draw(frame, &world.app))
        .unwrap();
    terminal.backend().buffer().clone()
}

fn marked(buffer: &Buffer, column: u16, row: u16) -> bool {
    buffer[(column, row)].modifier.contains(Modifier::REVERSED)
}

#[test]
fn a_drag_copies_the_session_s_text_and_nothing_beside_it() {
    let (mut world, _) = open_with_output(5, false);
    // From line2's third letter to line3's fourth.
    let actions = drag(
        &mut world,
        (PANE_X + 2, PANE_Y + 1),
        (PANE_X + 3, PANE_Y + 2),
    );
    assert_eq!(copied(&actions), Some("ne2\nline"));
    assert!(input(&actions).is_empty(), "nothing reaches the program");
    assert!(render(&world.app).contains("Copied 2 lines."));

    // Marked as a terminal marks it, in reading order: the rest of the
    // first line, the start of the last.
    let buffer = drawn(&world);
    assert!(!marked(&buffer, PANE_X + 1, PANE_Y + 1));
    assert!(marked(&buffer, PANE_X + 2, PANE_Y + 1));
    assert!(marked(&buffer, PANE_X + 60, PANE_Y + 1));
    assert!(marked(&buffer, PANE_X, PANE_Y + 2));
    assert!(marked(&buffer, PANE_X + 3, PANE_Y + 2));
    assert!(!marked(&buffer, PANE_X + 4, PANE_Y + 2));
    assert!(!marked(&buffer, 5, PANE_Y + 1), "the list is never marked");
}

#[test]
fn a_click_copies_nothing_and_a_key_or_the_wheel_clears_what_is_marked() {
    let (mut world, id) = open_with_output(5, false);
    let at = (PANE_X + 2, PANE_Y + 1);
    assert_eq!(copied(&drag(&mut world, at, at)), None);
    assert_eq!(world.app.selection, None);

    drag(&mut world, at, (PANE_X + 4, PANE_Y + 1));
    assert!(world.app.selection.is_some());
    let typed = press(&mut world.app, KeyCode::Char('x'));
    assert_eq!(input(&typed), b"x");
    assert_eq!(world.app.selection, None);

    drag(&mut world, at, (PANE_X + 4, PANE_Y + 1));
    world.app.on_mouse(mouse(MouseEventKind::ScrollUp, 50, 10));
    assert_eq!(world.app.selection, None);
    assert_eq!(
        world.app.screens[&id].scrolled_back(),
        0,
        "five lines keep none"
    );

    drag(&mut world, at, (PANE_X + 4, PANE_Y + 1));
    // A click on the list gives it the keys, and takes the mark away.
    world.app.on_mouse(left(MouseEventKind::Down, 5, 2));
    assert_eq!(world.app.mode, Mode::Browse);
    assert_eq!(world.app.selection, None);
}

#[test]
fn a_drag_backwards_and_out_of_the_pane_copies_what_it_covers() {
    let (mut world, _) = open_with_output(5, false);
    // From line3's third letter up and left, over the list and the top bar:
    // kept to the screen's first cell.
    let actions = drag(&mut world, (PANE_X + 2, PANE_Y + 2), (10, 0));
    assert_eq!(copied(&actions), Some("line1\nline2\nlin"));
    assert!(render(&world.app).contains("Copied 3 lines."));
}

#[test]
fn blanks_that_end_a_line_are_left_out() {
    let (mut world, id) = open_with_output(0, false);
    world.app.on_frame(ServerFrame::Replay {
        session: id,
        seq: 0,
        data: b"spaced   \r\nnext\r\n".to_vec(),
        reset: true,
    });
    let actions = drag(&mut world, (PANE_X, PANE_Y), (PANE_X + 1, PANE_Y + 1));
    assert_eq!(copied(&actions), Some("spaced\nne"));
    // One line is just copied.
    let actions = drag(&mut world, (PANE_X, PANE_Y), (PANE_X + 50, PANE_Y));
    assert_eq!(copied(&actions), Some("spaced"));
    assert!(render(&world.app).contains("Copied."));
}

#[test]
fn what_is_scrolled_back_to_is_what_is_copied() {
    let (mut world, id) = open_with_output(100, false);
    world.app.on_mouse(mouse(MouseEventKind::ScrollUp, 50, 10));
    assert_eq!(world.app.screens[&id].scrolled_back(), 3);
    // 100 lines and the empty one after them, 26 shown, 3 back: line73
    // at the top.
    let actions = drag(&mut world, (PANE_X, PANE_Y), (PANE_X + 5, PANE_Y));
    assert_eq!(copied(&actions), Some("line73"));
}

#[test]
fn a_program_that_asked_for_the_mouse_gets_it_but_with_shift_the_drag_selects() {
    let (mut world, id) = open_with_output(5, false);
    set_modes(&mut world, id, mouse_mode());
    let press = world
        .app
        .on_mouse(left(MouseEventKind::Down, PANE_X, PANE_Y));
    assert_eq!(input(&press), b"\x1b[<0;1;1M");
    world.app.on_mouse(left(MouseEventKind::Up, PANE_X, PANE_Y));
    assert_eq!(world.app.selection, None);

    let shifted = |kind, column, row| MouseEvent {
        modifiers: KeyModifiers::SHIFT,
        ..left(kind, column, row)
    };
    let mut actions = world
        .app
        .on_mouse(shifted(MouseEventKind::Down, PANE_X, PANE_Y));
    actions.extend(
        world
            .app
            .on_mouse(shifted(MouseEventKind::Drag, PANE_X + 4, PANE_Y)),
    );
    // Shift let go before the button: still the selection's.
    actions.extend(
        world
            .app
            .on_mouse(left(MouseEventKind::Up, PANE_X + 4, PANE_Y)),
    );
    assert!(input(&actions).is_empty(), "{actions:?}");
    assert_eq!(copied(&actions), Some("line1"));
}

#[test]
fn full_screen_the_screen_starts_below_the_top_bar() {
    let (mut world, _) = open_with_output(5, true);
    let actions = drag(&mut world, (0, 1), (4, 2));
    assert_eq!(copied(&actions), Some("line1\nline2"));
    let buffer = drawn(&world);
    assert!(marked(&buffer, 0, 1));
    assert!(marked(&buffer, 4, 2));
    assert!(!marked(&buffer, 5, 2));
    assert!(!marked(&buffer, 0, 3));
    let screen = render(&world.app);
    assert!(screen.contains("Copied 2 lines."), "{screen}");
}

#[test]
fn a_drag_over_the_preview_opens_the_session_and_selects() {
    let (mut world, id) = open_with_output(5, false);
    press(&mut world.app, KeyCode::Char('Q'));
    world.app.selected = Some(Row::Session(id));
    assert_eq!(world.app.mode, Mode::Browse);
    let actions = drag(&mut world, (PANE_X, PANE_Y), (PANE_X + 4, PANE_Y));
    assert_eq!(world.app.mode, Mode::Terminal(id));
    // The press opened it, and the drag selects from there.
    assert_eq!(copied(&actions), Some("line1"));
}

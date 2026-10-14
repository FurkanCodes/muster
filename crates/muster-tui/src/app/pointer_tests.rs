//! The mouse: scrolling back, reaching programs that ask for it, and
//! picking lines.

use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use muster_core::SessionId;
use muster_core::protocol::{
    ClientFrame, Delta, MouseEncoding, MouseTracking, ServerFrame, TerminalModes,
};

use super::tests::{World, press, render, world};
use super::{Action, Mode};
use crate::model::Row;

pub(super) fn mouse(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

/// The world's session open beside the list, with `lines` of output; full
/// screen with `full`.
pub(super) fn open_with_output(lines: usize, full: bool) -> (World, SessionId) {
    let mut world = world();
    let id = world.session.id;
    world.app.selected = Some(Row::Session(id));
    press(
        &mut world.app,
        if full {
            KeyCode::Char('f')
        } else {
            KeyCode::Enter
        },
    );
    assert_eq!(world.app.mode, Mode::Terminal(id));
    assert_eq!(world.app.full_screen, full);
    let output: String = (1..=lines).map(|n| format!("line{n}\r\n")).collect();
    world.app.on_frame(ServerFrame::Replay {
        session: id,
        seq: 0,
        data: output.into_bytes(),
        reset: true,
    });
    (world, id)
}

pub(super) fn set_modes(world: &mut World, session: SessionId, modes: TerminalModes) {
    world.app.on_frame(ServerFrame::Modes { session, modes });
}

pub(super) fn input(actions: &[Action]) -> Vec<u8> {
    actions
        .iter()
        .filter_map(|action| match action {
            Action::Send(ClientFrame::Input { data, .. }) => Some(data.clone()),
            _ => None,
        })
        .flatten()
        .collect()
}

pub(super) fn mouse_mode() -> TerminalModes {
    TerminalModes {
        mouse: MouseTracking::PressRelease,
        mouse_encoding: MouseEncoding::Sgr,
        ..TerminalModes::default()
    }
}

#[test]
fn the_wheel_scrolls_a_session_back_and_typing_returns() {
    let (mut world, id) = open_with_output(100, false);
    let actions = world.app.on_mouse(mouse(MouseEventKind::ScrollUp, 50, 10));
    assert!(input(&actions).is_empty(), "nothing reaches the program");
    assert_eq!(world.app.screens[&id].scrolled_back(), 3);
    let screen = render(&world.app);
    assert!(screen.contains("3 lines back"), "{screen}");

    world
        .app
        .on_mouse(mouse(MouseEventKind::ScrollDown, 50, 10));
    assert_eq!(world.app.screens[&id].scrolled_back(), 0);

    world.app.on_mouse(mouse(MouseEventKind::ScrollUp, 50, 10));
    let typed = press(&mut world.app, KeyCode::Char('x'));
    assert_eq!(input(&typed), b"x");
    assert_eq!(world.app.screens[&id].scrolled_back(), 0);
}

#[test]
fn beside_the_list_the_mouse_reaches_the_program_inside_its_pane() {
    let (mut world, id) = open_with_output(5, false);
    set_modes(&mut world, id, mouse_mode());
    // The pane starts after the 34-column list and its own border, below
    // the top bar: column 40, row 8 is the program's 5, 6; 1-based 6, 7.
    let actions = world.app.on_mouse(mouse(MouseEventKind::ScrollUp, 40, 8));
    assert_eq!(input(&actions), b"\x1b[<64;6;7M");
    let border = world
        .app
        .on_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 34, 8));
    assert!(input(&border).is_empty(), "the border is Muster's");
    assert_eq!(world.app.mode, Mode::Terminal(id));
}

#[test]
fn a_click_on_the_list_gives_it_the_keys_back() {
    let (mut world, id) = open_with_output(5, false);
    set_modes(&mut world, id, mouse_mode());
    let actions = world
        .app
        .on_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 5, 2));
    assert!(input(&actions).is_empty(), "the click is not the program's");
    assert_eq!(world.app.mode, Mode::Browse);
    assert_eq!(world.app.selected, Some(Row::Project(world.project.id)));
    // And a click on the session gives it the keys again.
    world.app.selected = Some(Row::Session(id));
    world
        .app
        .on_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 60, 10));
    assert_eq!(world.app.mode, Mode::Terminal(id));
}

#[test]
fn full_screen_the_program_gets_the_mouse_below_the_top_bar() {
    let (mut world, id) = open_with_output(5, true);
    set_modes(&mut world, id, mouse_mode());
    let actions = world.app.on_mouse(mouse(MouseEventKind::ScrollUp, 4, 7));
    // Row 7 on screen is row 6 of the program's, 1-based 7.
    assert_eq!(input(&actions), b"\x1b[<64;5;7M");
    let click = world
        .app
        .on_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 0, 1));
    assert_eq!(input(&click), b"\x1b[<0;1;1M");
    assert_eq!(world.app.mode, Mode::Terminal(id), "no list to click");
    // The top bar is not the program's: a click there gives the list the
    // keys back, as the list does beside a session.
    let top = world
        .app
        .on_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 3, 0));
    assert!(input(&top).is_empty());
    assert_eq!(world.app.mode, Mode::Browse);
    assert!(!world.app.full_screen);
    assert_eq!(world.app.screens[&id].scrolled_back(), 0);
}

#[test]
fn on_the_alternate_screen_the_wheel_sends_arrow_keys() {
    let (mut world, id) = open_with_output(5, false);
    set_modes(
        &mut world,
        id,
        TerminalModes {
            alternate_screen: true,
            application_cursor: true,
            ..TerminalModes::default()
        },
    );
    let actions = world.app.on_mouse(mouse(MouseEventKind::ScrollDown, 40, 8));
    assert_eq!(input(&actions), b"\x1bOB\x1bOB\x1bOB");
}

#[test]
fn in_the_list_the_wheel_moves_and_a_click_selects_or_opens() {
    let mut world = world();
    let id = world.session.id;
    assert_eq!(world.app.selected, Some(Row::Project(world.project.id)));
    // The list starts below the top bar and its border.
    world.app.on_mouse(mouse(MouseEventKind::ScrollDown, 3, 5));
    assert_eq!(world.app.selected, Some(Row::Worktree(world.worktree.id)));
    world
        .app
        .on_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 5, 4));
    assert_eq!(world.app.selected, Some(Row::Session(id)));
    // Below the last line: nothing changes.
    world
        .app
        .on_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 5, 20));
    assert_eq!(world.app.selected, Some(Row::Session(id)));
    // A click on the preview opens the selected session.
    world
        .app
        .on_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 60, 10));
    assert_eq!(world.app.mode, Mode::Terminal(id));
}

#[test]
fn clicks_follow_a_long_list_scrolled_to_its_selection() {
    let mut world = world();
    for n in 0..40 {
        let session = crate::model::tests::session(world.worktree.id, &format!("S{n}"), 10 + n);
        world
            .app
            .on_frame(ServerFrame::Delta(Delta::SessionUpserted(session)));
    }
    let rows = world.app.model.rows();
    // The whole terminal (the session area and the top bar), less the top
    // bar, the hints and the list's borders.
    let height = (29 + 1) - 1 - 1 - 2;
    // Walk down far, so the list scrolls to keep the selection in view.
    for _ in 0..35 {
        press(&mut world.app, KeyCode::Down);
    }
    assert_eq!(world.app.selected, Some(rows[35]));
    let first_shown = 35 - (height - 1);
    let label = |world: &World, row: Row| match row {
        Row::Session(id) => world.app.model.session(id).unwrap().title.clone(),
        _ => String::new(),
    };
    let top_line = render(&world.app).lines().nth(2).unwrap().to_owned();
    assert!(
        top_line.contains(&label(&world, rows[first_shown])),
        "{top_line}"
    );

    // A click on the top line selects what is drawn there, and the view
    // stays where it is.
    world
        .app
        .on_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 5, 2));
    assert_eq!(world.app.selected, Some(rows[first_shown]));
    let top_line = render(&world.app).lines().nth(2).unwrap().to_owned();
    assert!(
        top_line.contains(&label(&world, rows[first_shown])),
        "{top_line}"
    );
    // Moving up past the top scrolls one line, not back to the start.
    press(&mut world.app, KeyCode::Up);
    let top_line = render(&world.app).lines().nth(2).unwrap().to_owned();
    assert!(
        top_line.contains(&label(&world, rows[first_shown - 1])),
        "{top_line}"
    );
}

#[test]
fn page_keys_scroll_the_preview() {
    let mut world = world();
    let id = world.session.id;
    world.app.selected = Some(Row::Session(id));
    let output: String = (1..=200).map(|n| format!("line{n}\r\n")).collect();
    world.app.on_frame(ServerFrame::Replay {
        session: id,
        seq: 0,
        data: output.into_bytes(),
        reset: true,
    });
    press(&mut world.app, KeyCode::PageUp);
    let back = world.app.screens[&id].scrolled_back();
    assert!(back > 10, "{back}");
    assert!(render(&world.app).contains("lines back"));
    press(&mut world.app, KeyCode::End);
    assert_eq!(world.app.screens[&id].scrolled_back(), 0);
}

#[test]
fn a_message_taking_lines_from_the_list_keeps_clicks_on_what_is_drawn() {
    let mut world = world();
    for n in 0..40 {
        let session = crate::model::tests::session(world.worktree.id, &format!("S{n}"), 10 + n);
        world
            .app
            .on_frame(ServerFrame::Delta(Delta::SessionUpserted(session)));
    }
    for _ in 0..30 {
        press(&mut world.app, KeyCode::Down);
    }
    let selected = world.app.selected.unwrap();
    // A three-line reason makes the list two lines shorter.
    world.app.on_disconnected("why ".repeat(70));
    let screen = render(&world.app);
    let label = match selected {
        Row::Session(id) => world.app.model.session(id).unwrap().title.clone(),
        _ => unreachable!(),
    };
    assert!(
        screen
            .lines()
            .any(|line| line.contains(&format!("○ {label} "))),
        "the selection is still drawn:\n{screen}"
    );
    // And a click on the list's top line selects what is drawn there.
    let top = screen.lines().nth(2).unwrap().to_owned();
    world
        .app
        .on_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 5, 2));
    let clicked = match world.app.selected.unwrap() {
        Row::Session(id) => world.app.model.session(id).unwrap().title.clone(),
        other => panic!("{other:?}"),
    };
    assert!(top.contains(&format!("○ {clicked} ")), "{top} vs {clicked}");
}

#[test]
fn while_a_session_has_the_keys_the_wheel_scrolls_it_wherever_the_pointer_is() {
    let (mut world, id) = open_with_output(100, false);
    // Over the list, the pane's border and the bottom bar.
    for (column, row) in [(5, 4), (34, 1), (60, 29)] {
        world
            .app
            .on_mouse(mouse(MouseEventKind::ScrollUp, column, row));
    }
    assert_eq!(world.app.screens[&id].scrolled_back(), 9);
    assert_eq!(world.app.mode, Mode::Terminal(id), "the keys stay");
    // A program that asked for the mouse gets it at the screen's edge.
    set_modes(&mut world, id, mouse_mode());
    let actions = world.app.on_mouse(mouse(MouseEventKind::ScrollUp, 5, 4));
    assert_eq!(input(&actions), b"\x1b[<64;1;3M");
}

#[test]
fn a_click_on_a_project_s_details_opens_no_dialog() {
    let mut world = world();
    for row in [
        Row::Project(world.project.id),
        Row::Worktree(world.worktree.id),
    ] {
        world.app.selected = Some(row);
        world
            .app
            .on_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 60, 10));
        assert_eq!(world.app.mode, Mode::Browse, "{row:?}");
    }
    // A session's pane still opens it.
    world.app.selected = Some(Row::Session(world.session.id));
    world
        .app
        .on_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 60, 10));
    assert_eq!(world.app.mode, Mode::Terminal(world.session.id));
}

#[test]
fn over_the_preview_a_program_that_takes_the_mouse_scrolls_its_own_view() {
    let (mut world, id) = open_with_output(100, false);
    press(&mut world.app, KeyCode::Char('Q'));
    world.app.selected = Some(Row::Session(id));
    set_modes(&mut world, id, mouse_mode());
    let actions = world.app.on_mouse(mouse(MouseEventKind::ScrollUp, 40, 8));
    assert_eq!(input(&actions), b"\x1b[<64;6;7M");
    assert_eq!(world.app.screens[&id].scrolled_back(), 0);
    assert_eq!(world.app.mode, Mode::Browse, "the list keeps the keys");
    // Others scroll what Muster kept of them.
    set_modes(&mut world, id, TerminalModes::default());
    let actions = world.app.on_mouse(mouse(MouseEventKind::ScrollUp, 40, 8));
    assert!(input(&actions).is_empty());
    assert_eq!(world.app.screens[&id].scrolled_back(), 3);
}

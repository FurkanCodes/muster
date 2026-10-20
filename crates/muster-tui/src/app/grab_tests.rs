//! What the pointer could take hold of (`PRODUCT.md` §6 Sessions): the
//! line between two panels and a panel's title, lit as the pointer passes
//! over them; the cell before a line takes it too, moving it once the
//! pointer has gone two cells, a press there that does not move being the
//! click it was; and the session's first column stays its own, for
//! selecting its text.

use crossterm::event::{MouseButton, MouseEventKind};
use muster_core::layout::{Layout, Panel};
use muster_core::protocol::{ClientFrame, Command, MouseTracking, TerminalModes};
use muster_core::settings::PreferenceChange;
use ratatui::layout::Rect;

use super::grab::Grab;
use super::look_tests::buffer;
use super::pointer_tests::{input, mouse, mouse_mode, open_with_output, set_modes};
use super::tests::{World, world};
use super::{Action, Mode};
use crate::model::Row;

fn shown() -> World {
    let mut world = world();
    world.app.selected = Some(Row::Session(world.session.id));
    world.app.settle();
    world
}

fn moved(world: &mut World, at: (u16, u16)) -> Vec<Action> {
    world.app.on_mouse(mouse(MouseEventKind::Moved, at.0, at.1))
}

fn left(world: &mut World, kind: fn(MouseButton) -> MouseEventKind, at: (u16, u16)) -> Vec<Action> {
    world
        .app
        .on_mouse(mouse(kind(MouseButton::Left), at.0, at.1))
}

fn list(world: &World) -> Rect {
    let size = world.app.terminal_size;
    crate::view::browse_areas(&world.app, Rect::new(0, 0, size.cols, size.rows + 1)).list
}

/// Whether the cell at `at` is lit: on the accent's shade, which no
/// border is, the one with the keys being the accent itself.
fn lit(world: &World, at: (u16, u16)) -> bool {
    let theme = crate::view::theme::of(&world.app);
    let cell = &buffer(&world.app)[at];
    cell.fg == theme.accent && cell.bg == theme.lit
}

#[test]
fn the_line_between_two_panels_and_a_title_light_up_under_the_pointer() {
    let mut world = shown();
    // The list is 30 columns: the line is its edge and the session's, 29
    // and 30, and the cell before it, 28.
    for column in [28, 29, 30] {
        assert!(moved(&mut world, (column, 10)).is_empty());
        assert!(
            matches!(world.app.hover, Some(Grab::Line(_))),
            "at {column}"
        );
        assert!(
            lit(&world, (29, 10)) && lit(&world, (30, 20)),
            "at {column}"
        );
    }
    moved(&mut world, (5, 1));
    assert_eq!(world.app.hover, Some(Grab::Title(Panel::List)));
    assert!(lit(&world, (12, 1)), "the list's title row");
    assert!(!lit(&world, (29, 10)), "the line no longer");

    // The session's first column is for selecting its text: nothing to
    // take hold of there, nor inside a panel.
    for at in [(31, 10), (50, 10), (10, 8)] {
        moved(&mut world, at);
        assert_eq!(world.app.hover, None, "at {at:?}");
    }
}

#[test]
fn the_cell_before_a_line_moves_it_once_the_pointer_has_gone_two_cells() {
    let mut world = shown();
    left(&mut world, MouseEventKind::Down, (28, 10));
    left(&mut world, MouseEventKind::Drag, (29, 10));
    assert_eq!(list(&world).width, 30, "a hand that moves one cell");
    left(&mut world, MouseEventKind::Drag, (38, 10));
    assert_eq!(list(&world).width, 40, "as far as the pointer went");
    let actions = left(&mut world, MouseEventKind::Up, (38, 10));
    assert!(actions.contains(&Action::Send(ClientFrame::Command {
        req_id: 1,
        command: Command::ChangePreference(PreferenceChange::Panels(
            Layout::parse("list 40 | session 60% | (changes / shell)").unwrap()
        )),
    })));
    assert_eq!(world.app.drag, None);
}

#[test]
fn a_press_beside_a_line_that_does_not_move_is_the_click_it_was() {
    let mut world = shown();
    let project = list(&world).y + 1;
    left(&mut world, MouseEventKind::Down, (28, project));
    let actions = left(&mut world, MouseEventKind::Up, (29, project));
    assert_eq!(world.app.drag, None);
    assert_eq!(world.app.selected, Some(Row::Project(world.project.id)));
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Send(ClientFrame::Command { .. }))),
        "nothing kept: {actions:?}"
    );
}

#[test]
fn a_program_that_asked_for_every_movement_gets_it_and_one_that_did_not_never() {
    let (mut world, id) = open_with_output(5, false);
    assert_eq!(world.app.mode, Mode::Terminal(id));
    set_modes(&mut world, id, mouse_mode());
    assert!(input(&moved(&mut world, (36, 8))).is_empty());
    set_modes(
        &mut world,
        id,
        TerminalModes {
            mouse: MouseTracking::AnyMotion,
            ..mouse_mode()
        },
    );
    assert_eq!(input(&moved(&mut world, (36, 8))), b"\x1b[<35;6;7M");
    // Moving never skips the welcome, nor takes the keys.
    world.app.welcome = true;
    moved(&mut world, (5, 1));
    assert!(world.app.welcome);
    assert_eq!(world.app.hover, None);
}

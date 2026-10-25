//! The scroll bars (`PRODUCT.md` §6): a long list, a terminal scrolled
//! back and the help show one on their frame's edge; its thumb dragged, or
//! the track clicked, goes there; and on the line between two panels a
//! drag sideways still sizes them.

use crossterm::event::{KeyCode, MouseButton, MouseEventKind};
use muster_core::protocol::{Delta, ServerFrame, TermSize};
use ratatui::layout::Rect;

use super::pointer_tests::{mouse, open_with_output};
use super::scrollbars::Bar;
use super::tests::{World, press, world};
use super::{Mode, View};
use crate::model::Row;
use crate::model::tests::session;

const SIZE: TermSize = TermSize {
    cols: 140,
    rows: 20,
};

fn bar(world: &World, which: Bar) -> (Rect, super::scrollbars::Extent) {
    world
        .app
        .bars()
        .into_iter()
        .find(|(bar, _, _)| *bar == which)
        .map(|(_, track, extent)| (track, extent))
        .unwrap_or_else(|| panic!("no {which:?} in {:?}", world.app.bars()))
}

fn drag(world: &mut World, column: u16, from: u16, to: u16) {
    world
        .app
        .on_mouse(mouse(MouseEventKind::Down(MouseButton::Left), column, from));
    world
        .app
        .on_mouse(mouse(MouseEventKind::Drag(MouseButton::Left), column, to));
    world
        .app
        .on_mouse(mouse(MouseEventKind::Up(MouseButton::Left), column, to));
}

/// The world with forty sessions, a list longer than the window.
fn long_list() -> World {
    let mut world = world();
    for at in 0..40 {
        world
            .app
            .on_frame(ServerFrame::Delta(Delta::SessionUpserted(session(
                world.worktree.id,
                &format!("Chat {at}"),
                10 + at,
            ))));
    }
    world.app.on_resize(SIZE);
    world.app.selected = Some(Row::Project(world.project.id));
    world.app.settle();
    world
}

#[test]
fn a_long_list_s_bar_moves_the_selection_and_the_list_with_it() {
    let mut world = long_list();
    assert!(world.app.bars().iter().all(|(bar, _, _)| *bar != Bar::Help));
    let (track, extent) = bar(&world, Bar::List);
    assert_eq!(extent.first, 0);
    // To the bottom: the last line selected, the list scrolled to it.
    drag(&mut world, track.x, track.y, track.bottom() - 1);
    let rows = world.app.rows();
    assert_eq!(world.app.selected, rows.last().copied());
    let (_, extent) = bar(&world, Bar::List);
    // Its last line in view: rows of two lines may leave one spare.
    assert!(extent.first + extent.shown >= extent.total, "{extent:?}");
    // A click on the top of the track: back near the start.
    world.app.on_mouse(mouse(
        MouseEventKind::Down(MouseButton::Left),
        track.x,
        track.y,
    ));
    world.app.on_mouse(mouse(
        MouseEventKind::Up(MouseButton::Left),
        track.x,
        track.y,
    ));
    assert_eq!(world.app.selected, rows.first().copied());
}

#[test]
fn on_the_line_beside_the_list_a_drag_sideways_still_sizes_it() {
    let mut world = long_list();
    let (track, _) = bar(&world, Bar::List);
    let size = world.app.terminal_size;
    let window = Rect::new(0, 0, size.cols, size.rows + 1);
    let before = crate::view::browse_areas(&world.app, window).list.width;
    let row = track.y + 3;
    world
        .app
        .on_mouse(mouse(MouseEventKind::Down(MouseButton::Left), track.x, row));
    world.app.on_mouse(mouse(
        MouseEventKind::Drag(MouseButton::Left),
        track.x + 6,
        row,
    ));
    world.app.on_mouse(mouse(
        MouseEventKind::Up(MouseButton::Left),
        track.x + 6,
        row,
    ));
    let after = crate::view::browse_areas(&world.app, window).list.width;
    assert!(after > before, "{before} to {after}");
}

#[test]
fn a_terminal_scrolled_back_shows_a_bar_that_brings_it_back() {
    let (mut world, id) = open_with_output(200, false);
    world.app.on_resize(SIZE);
    assert!(
        world
            .app
            .bars()
            .iter()
            .all(|(bar, _, _)| *bar != Bar::Terminal(id)),
        "none at the newest output"
    );
    world.app.on_mouse(mouse(MouseEventKind::ScrollUp, 80, 10));
    let (track, extent) = bar(&world, Bar::Terminal(id));
    assert!(extent.total > extent.shown);
    // To its top: as far back as it goes.
    drag(&mut world, track.x, track.bottom() - 1, track.y);
    let screen = &world.app.screens[&id];
    assert_eq!(screen.scrolled_back(), screen.history());
    // And to its foot: the newest output, the bar gone.
    drag(&mut world, track.x, track.y, track.bottom() - 1);
    assert_eq!(world.app.screens[&id].scrolled_back(), 0);
    assert_eq!(world.app.mode, Mode::Terminal(id));
}

#[test]
fn the_help_shows_a_bar_where_its_keys_are_more_than_show() {
    let mut world = world();
    world.app.on_resize(TermSize {
        cols: 120,
        rows: 16,
    });
    press(&mut world.app, KeyCode::Char('h'));
    assert!(matches!(world.app.mode, Mode::Help(_)));
    assert_eq!(world.app.view, View::Sessions);
    let (track, extent) = bar(&world, Bar::Help);
    drag(&mut world, track.x, track.y, track.bottom() - 1);
    let Mode::Help(help) = &world.app.mode else {
        panic!("{:?}", world.app.mode);
    };
    assert_eq!(help.offset, extent.total - extent.shown);
}

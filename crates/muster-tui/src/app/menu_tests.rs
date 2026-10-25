//! A right-click on a line opens what can be done there, each with its
//! key, so the keys can be found without knowing them (`PRD` §6).

use crossterm::event::{KeyCode, MouseButton, MouseEventKind};
use ratatui::layout::Rect;

use super::pointer_tests::mouse;
use super::tests::{World, press, render, world};
use super::{Confirmed, Dialog, Mode};
use crate::model::Row;

const WINDOW: Rect = Rect {
    x: 0,
    y: 0,
    width: 100,
    height: 30,
};

/// Clicks `button` on the list's `line` (the first is 0), at its fifth
/// cell.
fn click_line(world: &mut World, button: MouseButton, line: u16) {
    let list = crate::view::browse_areas(&world.app, WINDOW).list;
    world.app.on_mouse(mouse(
        MouseEventKind::Down(button),
        list.x + 5,
        list.y + 1 + line,
    ));
}

/// The open menu's items, as they read.
fn items(world: &World) -> Vec<String> {
    match &world.app.mode {
        Mode::Menu(menu) => menu
            .items
            .iter()
            .map(|item| format!("{} {}", item.key_name(), item.what))
            .collect(),
        other => panic!("no menu open: {other:?}"),
    }
}

#[test]
fn a_right_click_on_a_session_opens_what_can_be_done_there_with_the_keys() {
    let mut world = world();
    click_line(&mut world, MouseButton::Right, 2);
    assert_eq!(world.app.selected, Some(Row::Session(world.session.id)));
    assert_eq!(
        items(&world),
        [
            "⏎ type in it",
            "f full screen",
            "d changes",
            "Shift-T shell",
            "Shift-R rename",
            "x close",
            "s stop",
            "Shift-P pin to the right",
            "n session"
        ]
    );
    let screen = render(&world.app);
    assert!(screen.contains(" f  full screen "), "{screen}");
    assert!(screen.contains("esc close"), "{screen}");
}

#[test]
fn a_right_click_on_a_worktree_or_a_project_offers_theirs() {
    let mut world = world();
    click_line(&mut world, MouseButton::Right, 1);
    let worktree = items(&world);
    for wanted in ["⏎ session", "x remove", "d changes", "t tidy", "e fold"] {
        assert!(worktree.iter().any(|item| item == wanted), "{worktree:?}");
    }
    press(&mut world.app, KeyCode::Esc);
    click_line(&mut world, MouseButton::Right, 0);
    assert_eq!(
        items(&world),
        ["⏎ worktree", "e fold", "n session", "i tickets", "x remove"]
    );
}

#[test]
fn choosing_an_item_does_what_its_key_does() {
    let mut world = world();
    click_line(&mut world, MouseButton::Right, 2);
    // ↓ to full screen, and ⏎.
    press(&mut world.app, KeyCode::Down);
    press(&mut world.app, KeyCode::Enter);
    assert_eq!(world.app.mode, Mode::Terminal(world.session.id));
    assert!(world.app.full_screen);
}

#[test]
fn an_item_s_own_key_or_a_click_on_it_does_it() {
    let mut world = world();
    click_line(&mut world, MouseButton::Right, 2);
    press(&mut world.app, KeyCode::Char('x'));
    assert!(matches!(
        world.app.mode,
        Mode::Dialog(Dialog::Confirm {
            then: Confirmed::CloseSession(_),
            ..
        })
    ));

    let mut world = super::tests::world();
    click_line(&mut world, MouseButton::Right, 2);
    let Mode::Menu(menu) = world.app.mode.clone() else {
        panic!("no menu");
    };
    let area = crate::view::menu_area(&menu, WINDOW);
    // The second item: full screen.
    world.app.on_mouse(mouse(
        MouseEventKind::Down(MouseButton::Left),
        area.x + 2,
        area.y + 2,
    ));
    assert_eq!(world.app.mode, Mode::Terminal(world.session.id));
    assert!(world.app.full_screen);
}

#[test]
fn esc_or_a_click_outside_closes_it_and_does_nothing_else() {
    let mut world = world();
    click_line(&mut world, MouseButton::Right, 2);
    press(&mut world.app, KeyCode::Esc);
    assert_eq!(world.app.mode, Mode::Browse);

    click_line(&mut world, MouseButton::Right, 2);
    // A click on the project's line, outside the menu: it only closes.
    click_line(&mut world, MouseButton::Left, 0);
    assert_eq!(world.app.mode, Mode::Browse);
    assert_eq!(world.app.selected, Some(Row::Session(world.session.id)));
}

#[test]
fn a_right_click_on_the_list_beside_a_session_gives_the_list_the_keys() {
    let mut world = world();
    world.app.selected = Some(Row::Session(world.session.id));
    press(&mut world.app, KeyCode::Enter);
    assert_eq!(world.app.mode, Mode::Terminal(world.session.id));
    click_line(&mut world, MouseButton::Right, 1);
    assert_eq!(world.app.selected, Some(Row::Worktree(world.worktree.id)));
    assert!(matches!(world.app.mode, Mode::Menu(_)));
}

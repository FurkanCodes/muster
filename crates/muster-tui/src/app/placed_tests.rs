//! Sessions placed in panels of their own (`PRODUCT.md` §6 Sessions): `P`,
//! and *pin to the right* on a line's menu, place the selected session at
//! the right of the panel with the keys and again take it off the screen, as
//! its title's `✕` does; the preview's `◆`
//! places the session it shows and its `✕` folds it away; a session's
//! line dragged out of the list lands where it is dropped, and a panel
//! dropped on another's middle swaps with it; each is streamed, drawn and
//! typed in on its own.

use crossterm::event::{KeyCode, MouseButton, MouseEventKind};
use muster_core::SessionId;
use muster_core::layout::{Edge, Panel, Place};
use muster_core::protocol::{ClientFrame, Command, Delta, ServerFrame, TermSize};
use muster_core::settings::PreferenceChange;
use ratatui::layout::Rect;

use super::pointer_tests::mouse;
use super::tests::{World, press, render_in, world};
use super::{Action, Mode};
use crate::model::Row;
use crate::model::tests::session;
use crate::view::pane::Button;

const WIDE: TermSize = TermSize {
    cols: 180,
    rows: 29,
};

/// The world in a wide window, with a second session, Claude.
fn two() -> (World, SessionId) {
    let mut world = world();
    let second = session(world.worktree.id, "Claude", 4);
    let id = second.id;
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::SessionUpserted(second)));
    world.app.on_resize(WIDE);
    (world, id)
}

/// The layout `actions` ask the daemon to keep, if they do.
fn kept(actions: &[Action]) -> Option<muster_core::layout::Layout> {
    actions.iter().find_map(|action| match action {
        Action::Send(ClientFrame::Command {
            command: Command::ChangePreference(PreferenceChange::Panels(layout)),
            ..
        }) => Some(layout.clone()),
        _ => None,
    })
}

/// The sessions `actions` start streaming.
fn attached(actions: &[Action]) -> Vec<SessionId> {
    actions
        .iter()
        .filter_map(|action| match action {
            Action::Send(ClientFrame::Attach { session, .. }) => Some(*session),
            _ => None,
        })
        .collect()
}

fn area_of(world: &World, session: SessionId) -> Rect {
    let size = world.app.terminal_size;
    crate::view::browse_areas(&world.app, Rect::new(0, 0, size.cols, size.rows + 1))
        .placed
        .into_iter()
        .find(|(placed, _)| *placed == session)
        .map_or(Rect::default(), |(_, area)| area)
}

#[test]
fn shift_p_places_the_selected_session_at_the_right_and_again_takes_it_off() {
    let (mut world, second) = two();
    let first = world.session.id;
    world.app.selected = Some(Row::Session(first));
    world.app.settle();
    let actions = press(&mut world.app, KeyCode::Char('P'));
    let layout = kept(&actions).expect("the daemon keeps it");
    assert_eq!(layout.placed(), [first]);
    assert_eq!(
        layout,
        muster_core::layout::Layout::default()
            .placing(first, Place::Beside(Panel::Session, Edge::Right)),
        "at the right of the preview"
    );
    assert_eq!(world.app.panels().placed(), [first], "shown at once");
    assert!(!area_of(&world, first).is_empty());
    // Drawn once, in its own panel: the session's panel, with nothing else
    // to show, goes.
    let size = world.app.terminal_size;
    let areas = crate::view::browse_areas(&world.app, Rect::new(0, 0, size.cols, size.rows + 1));
    assert!(areas.preview.is_empty(), "{areas:?}");

    // Another selected: both are on screen, each streamed.
    world.app.selected = Some(Row::Session(second));
    let actions = world.app.settle();
    assert!(attached(&actions).contains(&second), "{actions:?}");
    let screen = render_in(&world.app, WIDE);
    assert!(screen.contains("Claude"), "{screen}");
    assert!(!area_of(&world, first).is_empty());

    // Again on the one pinned: unpinned.
    world.app.selected = Some(Row::Session(first));
    world.app.settle();
    let actions = press(&mut world.app, KeyCode::Char('P'));
    assert!(kept(&actions).expect("kept").placed().is_empty());
    assert!(world.app.panels().placed().is_empty());
}

#[test]
fn a_session_s_line_dragged_out_of_the_list_is_placed_where_it_is_dropped() {
    let (mut world, second) = two();
    // The first session shown beside the list.
    world.app.selected = Some(Row::Session(world.session.id));
    world.app.settle();
    let down = |kind| mouse(kind, 8, 6);
    // Pressed, it is selected at once, as a click selects.
    world
        .app
        .on_mouse(down(MouseEventKind::Down(MouseButton::Left)));
    assert_eq!(world.app.selected, Some(Row::Session(second)));
    // Let go in the list: nothing is pinned.
    world
        .app
        .on_mouse(mouse(MouseEventKind::Drag(MouseButton::Left), 9, 7));
    let actions = world
        .app
        .on_mouse(mouse(MouseEventKind::Up(MouseButton::Left), 9, 7));
    assert_eq!(kept(&actions), None);

    // Dragged to the window's right edge: pinned there.
    world
        .app
        .on_mouse(down(MouseEventKind::Down(MouseButton::Left)));
    world
        .app
        .on_mouse(mouse(MouseEventKind::Drag(MouseButton::Left), 179, 12));
    let actions = world
        .app
        .on_mouse(mouse(MouseEventKind::Up(MouseButton::Left), 179, 12));
    let layout = kept(&actions).expect("pinned");
    assert_eq!(
        layout,
        muster_core::layout::Layout::default().placing(second, Place::Window(Edge::Right))
    );
    let area = area_of(&world, second);
    assert_eq!(area.right(), 180, "{area:?}");
    // The session shown before it was pressed stays beside the list, so
    // both terminals are drawn and streamed.
    world.app.settle();
    assert_eq!(world.app.session_beside_list(), Some(world.session.id));
    assert_eq!(world.app.shown_session(), Some(world.session.id));
    let size = world.app.terminal_size;
    let areas = crate::view::browse_areas(&world.app, Rect::new(0, 0, size.cols, size.rows + 1));
    assert!(!areas.preview.is_empty(), "{areas:?}");
    let screen = render_in(&world.app, WIDE);
    assert!(!screen.contains("It is in a panel of its own."), "{screen}");
}

#[test]
fn pin_to_the_right_on_a_line_s_menu_places_it_there_and_again_takes_it_off() {
    let (mut world, second) = two();
    world.app.selected = Some(Row::Session(world.session.id));
    world.app.settle();
    let choose = |world: &mut World, wanted: &str| {
        // A right-click on the second session's line, then its item.
        world
            .app
            .on_mouse(mouse(MouseEventKind::Down(MouseButton::Right), 8, 6));
        let Mode::Menu(menu) = &world.app.mode else {
            panic!("no menu: {:?}", world.app.mode);
        };
        let at = menu
            .items
            .iter()
            .position(|item| item.what == wanted)
            .unwrap_or_else(|| panic!("no {wanted:?} in {:?}", menu.items));
        for _ in 0..at {
            press(&mut world.app, KeyCode::Down);
        }
        press(&mut world.app, KeyCode::Enter)
    };
    let actions = choose(&mut world, "pin to the right");
    assert_eq!(world.app.selected, Some(Row::Session(second)));
    assert_eq!(
        kept(&actions).expect("pinned"),
        muster_core::layout::Layout::default()
            .placing(second, Place::Beside(Panel::Session, Edge::Right))
    );
    // Pinned, its menu takes it off the screen.
    let actions = choose(&mut world, "off the screen");
    assert!(kept(&actions).expect("kept").placed().is_empty());
}

#[test]
fn a_click_on_a_session_pinned_gives_it_the_keys_and_typing_goes_to_it() {
    let (mut world, second) = two();
    world.app.selected = Some(Row::Session(world.session.id));
    world.app.settle();
    let pinned = world
        .app
        .panels()
        .placing(second, Place::Window(Edge::Right));
    world.app.panels.kept = Some(pinned);
    world.app.settle();
    let area = area_of(&world, second);
    world.app.on_mouse(mouse(
        MouseEventKind::Down(MouseButton::Left),
        area.x + 5,
        area.y + 5,
    ));
    assert_eq!(world.app.mode, Mode::Terminal(second));
    let actions = press(&mut world.app, KeyCode::Char('x'));
    assert!(
        actions.iter().any(|action| matches!(
            action,
            Action::Send(ClientFrame::Input { session, .. }) if *session == second
        )),
        "{actions:?}"
    );
    // The session beside the list is the one selected still.
    assert_eq!(world.app.focused_session(), Some(world.session.id));
}

/// The world's areas in its window.
fn areas(world: &World) -> crate::view::BrowseAreas {
    let size = world.app.terminal_size;
    crate::view::browse_areas(&world.app, Rect::new(0, 0, size.cols, size.rows + 1))
}

/// A click at `column`, `row`: pressed and let go there.
fn click(world: &mut World, column: u16, row: u16) -> Vec<Action> {
    let mut actions =
        world
            .app
            .on_mouse(mouse(MouseEventKind::Down(MouseButton::Left), column, row));
    actions.extend(
        world
            .app
            .on_mouse(mouse(MouseEventKind::Up(MouseButton::Left), column, row)),
    );
    actions
}

/// Where `button` is in `panel`'s title.
fn button(world: &World, panel: Panel, button: Button) -> (u16, u16) {
    let area = areas(world).of(panel);
    let (_, cells) = crate::view::pane::button_areas(&world.app, panel, area)
        .into_iter()
        .find(|(found, _)| *found == button)
        .unwrap_or_else(|| panic!("no {button:?} on {panel:?} at {area:?}"));
    (cells.x + 1, cells.y)
}

#[test]
fn a_placed_panel_s_cross_takes_it_off_the_screen_its_session_running_on() {
    let (mut world, second) = two();
    world.app.selected = Some(Row::Session(world.session.id));
    world.app.settle();
    world.app.place_at(second, Place::Window(Edge::Right));
    world.app.mode = Mode::Terminal(second);
    let screen = render_in(&world.app, WIDE);
    assert!(screen.contains(" ✕ "), "{screen}");
    let (column, row) = button(&world, Panel::Placed(second), Button::TakeOff);
    let actions = click(&mut world, column, row);
    assert!(kept(&actions).expect("kept").placed().is_empty());
    assert!(world.app.panels().placed().is_empty());
    assert_eq!(world.app.mode, Mode::Browse, "the keys back to the list");
    assert!(world.app.model.session(second).is_some(), "it runs on");
    assert!(
        !actions.iter().any(|action| matches!(
            action,
            Action::Send(ClientFrame::Command {
                command: Command::StopSession { .. } | Command::RemoveSession { .. },
                ..
            })
        )),
        "{actions:?}"
    );
}

#[test]
fn the_preview_s_diamond_places_its_session_where_the_preview_is() {
    let (mut world, second) = two();
    world.app.selected = Some(Row::Session(second));
    world.app.settle();
    let preview = areas(&world).preview;
    let (column, row) = button(&world, Panel::Session, Button::Place);
    let actions = click(&mut world, column, row);
    assert_eq!(kept(&actions).expect("kept").placed(), [second]);
    let placed = area_of(&world, second);
    assert_eq!(placed.x, preview.x, "where the preview was: {placed:?}");
}

#[test]
fn the_preview_s_cross_folds_it_away_until_another_is_selected() {
    let (mut world, second) = two();
    let first = world.session.id;
    world.app.selected = Some(Row::Session(first));
    world.app.settle();
    let (column, row) = button(&world, Panel::Session, Button::TakeOff);
    let actions = click(&mut world, column, row);
    assert_eq!(kept(&actions), None, "the layout stays as it is");
    assert!(areas(&world).preview.is_empty());
    assert_eq!(world.app.shown_session(), None, "nothing streamed for it");
    // Another selected: the preview shows it.
    world.app.selected = Some(Row::Session(second));
    world.app.settle();
    assert!(!areas(&world).preview.is_empty());
    // And the first again: folded no more.
    world.app.selected = Some(Row::Session(first));
    world.app.settle();
    assert!(!areas(&world).preview.is_empty());
}

#[test]
fn a_panel_dropped_on_another_s_middle_swaps_places_with_it() {
    let (mut world, second) = two();
    world.app.selected = Some(Row::Session(world.session.id));
    world.app.settle();
    world.app.place_at(second, Place::Window(Edge::Right));
    let before = world.app.panels().clone();
    let placed = area_of(&world, second);
    let preview = areas(&world).preview;
    // Taken by its title, carried to the middle of the preview.
    let (column, row) = (placed.x + 4, placed.y);
    world
        .app
        .on_mouse(mouse(MouseEventKind::Down(MouseButton::Left), column, row));
    let middle = (
        preview.x + preview.width / 2,
        preview.y + preview.height / 2,
    );
    world.app.on_mouse(mouse(
        MouseEventKind::Drag(MouseButton::Left),
        middle.0,
        middle.1,
    ));
    let actions = world.app.on_mouse(mouse(
        MouseEventKind::Up(MouseButton::Left),
        middle.0,
        middle.1,
    ));
    assert_eq!(
        kept(&actions).expect("kept"),
        before.swapped(Panel::Placed(second), Panel::Session)
    );
    assert_eq!(
        area_of(&world, second).x,
        preview.x,
        "in the preview's place"
    );
}

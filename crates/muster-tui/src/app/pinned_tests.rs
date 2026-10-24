//! Sessions pinned in panels of their own (`PRODUCT.md` §6 Sessions): `p`
//! pins the selected session beside the panel with the keys and again
//! unpins it; a session's line dragged out of the list lands where it is
//! dropped; each is streamed, drawn and typed in on its own.

use crossterm::event::{KeyCode, MouseButton, MouseEventKind};
use muster_core::SessionId;
use muster_core::layout::{Edge, Place};
use muster_core::protocol::{ClientFrame, Command, Delta, ServerFrame, TermSize};
use muster_core::settings::PreferenceChange;
use ratatui::layout::Rect;

use super::pointer_tests::mouse;
use super::tests::{World, press, render_in, world};
use super::{Action, Mode};
use crate::model::Row;
use crate::model::tests::session;

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
        .pinned
        .into_iter()
        .find(|(pinned, _)| *pinned == session)
        .map_or(Rect::default(), |(_, area)| area)
}

#[test]
fn p_pins_the_selected_session_beside_and_again_unpins_it() {
    let (mut world, second) = two();
    let first = world.session.id;
    world.app.selected = Some(Row::Session(first));
    world.app.settle();
    let actions = press(&mut world.app, KeyCode::Char('p'));
    let layout = kept(&actions).expect("the daemon keeps it");
    assert_eq!(layout.pinned(), [first]);
    assert_eq!(world.app.panels().pinned(), [first], "shown at once");
    assert!(!area_of(&world, first).is_empty());
    // Drawn once, in its own panel: the session panel says so.
    let screen = render_in(&world.app, WIDE);
    assert!(screen.contains("It is in a panel of its own."), "{screen}");

    // Another selected: both are on screen, each streamed.
    world.app.selected = Some(Row::Session(second));
    let actions = world.app.settle();
    assert!(attached(&actions).contains(&second), "{actions:?}");
    let screen = render_in(&world.app, WIDE);
    assert!(
        screen.contains("Claude") && !screen.contains("It is in a panel of its own."),
        "{screen}"
    );
    assert!(!area_of(&world, first).is_empty());

    // Again on the one pinned: unpinned.
    world.app.selected = Some(Row::Session(first));
    world.app.settle();
    let actions = press(&mut world.app, KeyCode::Char('p'));
    assert!(kept(&actions).expect("kept").pinned().is_empty());
    assert!(world.app.panels().pinned().is_empty());
}

#[test]
fn a_session_s_line_dragged_out_of_the_list_is_pinned_where_it_is_dropped() {
    let (mut world, second) = two();
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
        muster_core::layout::Layout::default().pinning(second, Place::Window(Edge::Right))
    );
    let area = area_of(&world, second);
    assert_eq!(area.right(), 180, "{area:?}");
}

#[test]
fn a_click_on_a_session_pinned_gives_it_the_keys_and_typing_goes_to_it() {
    let (mut world, second) = two();
    world.app.selected = Some(Row::Session(world.session.id));
    world.app.settle();
    let pinned = world
        .app
        .panels()
        .pinning(second, Place::Window(Edge::Right));
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

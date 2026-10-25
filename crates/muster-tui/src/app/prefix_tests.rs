//! The panel key (`PRODUCT.md` §6 Sessions): Ctrl-B, on the list or
//! typing in a panel, then one key: split with a new shell, move the keys
//! and swap that way, zoom, take a panel off the screen or close its
//! session, resize, go to a panel by its number, or send Ctrl-B on; Esc
//! gives up; and a right-click on a title offers the same.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEventKind};
use muster_core::SessionId;
use muster_core::layout::{Edge, Panel, Place};
use muster_core::protocol::{ClientFrame, Command, Delta, ServerFrame, TermSize};
use muster_core::settings::PreferenceChange;

use super::pointer_tests::mouse;
use super::tests::{World, ctrl, key, render_in, world};
use super::{Action, Dialog, Mode};
use crate::model::Row;
use crate::model::tests::session;

const WIDE: TermSize = TermSize {
    cols: 200,
    rows: 29,
};

/// The world in a wide window: its session in the preview with the keys,
/// a second placed to its right.
fn two() -> (World, SessionId) {
    let mut world = world();
    let second = session(world.worktree.id, "Claude", 4);
    let id = second.id;
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::SessionUpserted(second)));
    world.app.on_resize(WIDE);
    world.app.selected = Some(Row::Session(world.session.id));
    world.app.settle();
    world
        .app
        .place_at(id, Place::Beside(Panel::Session, Edge::Right));
    world.app.mode = Mode::Terminal(world.session.id);
    world.app.settle();
    (world, id)
}

/// Ctrl-B, then `then`.
fn after_panel_key(world: &mut World, then: KeyEvent) -> Vec<Action> {
    let waiting = world.app.on_key(ctrl('b'));
    assert!(
        typed(&waiting).is_empty(),
        "nothing reaches the program: {waiting:?}"
    );
    world.app.on_key(then)
}

fn letter(c: char) -> KeyEvent {
    let shift = if c.is_ascii_uppercase() {
        KeyModifiers::SHIFT
    } else {
        KeyModifiers::NONE
    };
    KeyEvent::new(KeyCode::Char(c), shift)
}

fn areas(world: &World) -> crate::view::BrowseAreas {
    let size = world.app.terminal_size;
    crate::view::browse_areas(
        &world.app,
        ratatui::layout::Rect::new(0, 0, size.cols, size.rows + 1),
    )
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

/// What `actions` type into a session.
fn typed(actions: &[Action]) -> Vec<Vec<u8>> {
    actions
        .iter()
        .filter_map(|action| match action {
            Action::Send(ClientFrame::Input { data, .. }) => Some(data.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn the_panel_key_and_v_split_the_panel_right_with_a_new_shell() {
    let (mut world, _) = two();
    let actions = after_panel_key(&mut world, letter('v'));
    let (_, asked) = super::tests::command(&actions);
    let Command::StartSession {
        workspace, harness, ..
    } = asked
    else {
        panic!("{asked:?}");
    };
    assert_eq!((workspace, harness.as_str()), (world.worktree.id, "shell"));
    assert!(typed(&actions).is_empty());
    // And s, or -, down.
    let actions = after_panel_key(&mut world, letter('s'));
    assert!(matches!(
        super::tests::command(&actions).1,
        Command::StartSession { .. }
    ));
}

#[test]
fn esc_or_a_key_that_does_nothing_gives_up_and_nothing_is_typed() {
    let (mut world, _) = two();
    let layout = world.app.panels().clone();
    for then in [key(KeyCode::Esc), letter('q'), letter('Y')] {
        let actions = after_panel_key(&mut world, then);
        assert!(
            actions
                .iter()
                .all(|action| !matches!(action, Action::Send(_))),
            "{actions:?}"
        );
        assert_eq!(world.app.panels.waiting, None);
    }
    assert_eq!(*world.app.panels(), layout);
    // Typing goes to the program again.
    assert_eq!(typed(&world.app.on_key(letter('a'))), [b"a".to_vec()]);
}

#[test]
fn the_panel_key_twice_or_then_b_sends_it_to_the_program() {
    let (mut world, _) = two();
    assert_eq!(typed(&after_panel_key(&mut world, ctrl('b'))), [vec![0x02]]);
    assert_eq!(
        typed(&after_panel_key(&mut world, letter('b'))),
        [vec![0x02]]
    );
}

#[test]
fn the_keys_move_to_the_panel_that_way_by_letter_or_arrow() {
    let (mut world, second) = two();
    let first = world.session.id;
    after_panel_key(&mut world, letter('l'));
    assert_eq!(world.app.mode, Mode::Terminal(second));
    after_panel_key(&mut world, key(KeyCode::Left));
    assert_eq!(world.app.mode, Mode::Terminal(first));
    // Past the preview on the left, the list.
    after_panel_key(&mut world, letter('h'));
    assert_eq!(world.app.mode, Mode::Browse);
    // And from the list, back into the preview.
    after_panel_key(&mut world, letter('l'));
    assert_eq!(world.app.mode, Mode::Terminal(first));
    // w gives the keys back to the list.
    after_panel_key(&mut world, letter('w'));
    assert_eq!(world.app.mode, Mode::Browse);
}

#[test]
fn a_capital_or_shift_and_an_arrow_swaps_with_the_panel_that_way() {
    let (mut world, second) = two();
    let before = world.app.panels().clone();
    let actions = after_panel_key(&mut world, letter('L'));
    let swapped = before.swapped(Panel::Session, Panel::Placed(second));
    assert_eq!(kept(&actions), Some(swapped.clone()));
    let actions = after_panel_key(
        &mut world,
        KeyEvent::new(KeyCode::Left, KeyModifiers::SHIFT),
    );
    assert!(kept(&actions).is_some(), "{actions:?}");
}

#[test]
fn z_zooms_the_panel_over_the_others_and_the_keys_go_round_them() {
    let (mut world, second) = two();
    after_panel_key(&mut world, letter('z'));
    assert_eq!(world.app.panels.zoomed, Some(Panel::Session));
    let shown = areas(&world);
    assert!(!shown.preview.is_empty() && !shown.list.is_empty());
    assert!(shown.of(Panel::Placed(second)).is_empty());
    assert_eq!(shown.hidden, 1);
    let screen = render_in(&world.app, WIDE);
    assert!(screen.contains(" ZOOM "), "{screen}");
    assert!(screen.contains("+1 more"), "{screen}");
    // The keys to the next: it is the one zoomed.
    after_panel_key(&mut world, letter('l'));
    assert_eq!(world.app.mode, Mode::Terminal(second));
    assert_eq!(world.app.panels.zoomed, Some(Panel::Placed(second)));
    assert!(!areas(&world).of(Panel::Placed(second)).is_empty());
    // Again, put back.
    after_panel_key(&mut world, letter('z'));
    assert_eq!(world.app.panels.zoomed, None);
    assert_eq!(areas(&world).hidden, 0);
}

#[test]
fn x_takes_the_panel_off_the_screen_and_shift_x_closes_its_session_after_asking() {
    let (mut world, second) = two();
    world.app.mode = Mode::Terminal(second);
    let actions = after_panel_key(&mut world, letter('x'));
    assert!(kept(&actions).expect("kept").placed().is_empty());
    assert!(world.app.model.session(second).is_some(), "it runs on");
    world.app.mode = Mode::Terminal(world.session.id);
    after_panel_key(&mut world, letter('X'));
    assert!(
        matches!(&world.app.mode, Mode::Dialog(Dialog::Confirm { .. })),
        "{:?}",
        world.app.mode
    );
}

#[test]
fn r_resizes_the_panel_with_the_keys_and_esc_puts_it_back() {
    let (mut world, _) = two();
    let before = world.app.panels().clone();
    let width = areas(&world).preview.width;
    after_panel_key(&mut world, letter('r'));
    // Its right edge moves right: wider, by about five as the share of its
    // place rounds.
    world.app.on_key(letter('L'));
    let wider = areas(&world).preview.width;
    assert!(
        (width + 3..=width + 6).contains(&wider),
        "{width} to {wider}"
    );
    world.app.on_key(letter('H'));
    let back = areas(&world).preview.width;
    assert!(back < wider, "{wider} to {back}");
    let screen = render_in(&world.app, WIDE);
    assert!(screen.contains("move its edge"), "{screen}");
    world.app.on_key(key(KeyCode::Esc));
    assert_eq!(*world.app.panels(), before);
    // ⏎ keeps it.
    after_panel_key(&mut world, letter('r'));
    world.app.on_key(letter('l'));
    let actions = world.app.on_key(key(KeyCode::Enter));
    assert!(kept(&actions).is_some(), "{actions:?}");
    assert_eq!(world.app.panels.resizing, None);
}

#[test]
fn a_number_goes_to_the_panel_its_title_shows_while_the_panel_key_waits() {
    let (mut world, second) = two();
    world.app.on_key(ctrl('b'));
    let screen = render_in(&world.app, WIDE);
    assert!(screen.contains(" 1 "), "{screen}");
    assert!(screen.contains(" 2 Claude"), "{screen}");
    assert!(screen.contains("split →"), "the keys after it: {screen}");
    world.app.on_key(letter('2'));
    assert_eq!(world.app.mode, Mode::Terminal(second));
}

#[test]
fn a_right_click_on_a_title_offers_what_the_panel_key_does_to_it() {
    let (mut world, second) = two();
    let placed = areas(&world).of(Panel::Placed(second));
    world.app.on_mouse(mouse(
        MouseEventKind::Down(MouseButton::Right),
        placed.x + 4,
        placed.y,
    ));
    let Mode::Menu(menu) = world.app.mode.clone() else {
        panic!("{:?}", world.app.mode);
    };
    assert_eq!(menu.on, Some(Panel::Placed(second)));
    let items: Vec<String> = menu
        .items
        .iter()
        .map(|item| format!("{} {}", item.key, item.what))
        .collect();
    for wanted in [
        "Ctrl-b v split right, a shell",
        "Ctrl-b z zoom",
        "Ctrl-b x off the screen",
    ] {
        assert!(
            items.iter().any(|item| item == wanted),
            "{wanted} in {items:?}"
        );
    }
    // Its own key, the one after the panel key, does it.
    let actions = world.app.on_key(letter('x'));
    assert!(kept(&actions).expect("kept").placed().is_empty());
}

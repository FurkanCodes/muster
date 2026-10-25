//! The panels (`PRODUCT.md` §6 Sessions): the list as wide as its names
//! need, folded away on Ctrl+\, a workspace's changes beside the session,
//! and the panels moved by a drag or with the keys, kept in Muster's own
//! settings.

use crossterm::event::{KeyCode, MouseButton, MouseEventKind};
use muster_core::layout::{Edge, Layout, Place};
use muster_core::protocol::{ClientFrame, Command, Delta, Reply, ServerFrame, TermSize};
use muster_core::settings::PreferenceChange;
use muster_core::time::Timestamp;
use ratatui::layout::Rect;

use super::drag::Drag;
use super::pointer_tests::mouse;
use super::tests::{PANE, World, ctrl, preferences, press, world};
use super::{Action, Mode, SettingRow, View};
use crate::model::Row;
use crate::view::BrowseAreas;

const WIDE: TermSize = TermSize {
    cols: 180,
    rows: 29,
};

fn areas(world: &World) -> BrowseAreas {
    let size = world.app.terminal_size;
    crate::view::browse_areas(&world.app, Rect::new(0, 0, size.cols, size.rows + 1))
}

/// The world's session selected beside the list, and streamed.
fn shown() -> World {
    let mut world = world();
    world.app.selected = Some(Row::Session(world.session.id));
    world.app.settle();
    world
}

/// The change to Muster's own settings `actions` ask for, if any.
fn changed(actions: &[Action]) -> Option<(u64, PreferenceChange)> {
    actions.iter().find_map(|action| match action {
        Action::Send(ClientFrame::Command {
            req_id,
            command: Command::ChangePreference(change),
        }) => Some((*req_id, change.clone())),
        _ => None,
    })
}

/// The size `actions` give the session's program, if they give one.
fn resized(actions: &[Action]) -> Option<TermSize> {
    actions.iter().find_map(|action| match action {
        Action::Send(ClientFrame::Resize { size, .. }) => Some(*size),
        _ => None,
    })
}

fn left(world: &mut World, kind: fn(MouseButton) -> MouseEventKind, at: (u16, u16)) -> Vec<Action> {
    world
        .app
        .on_mouse(mouse(kind(MouseButton::Left), at.0, at.1))
}

fn done(world: &mut World, req_id: u64) {
    world.app.on_frame(ServerFrame::Reply {
        req_id,
        outcome: Ok(Reply::Done),
    });
}

#[test]
fn the_list_is_as_wide_as_its_names_need_and_no_wider_than_36_columns() {
    let mut world = shown();
    assert_eq!(
        areas(&world).list.width,
        30,
        "short names: as narrow as it goes"
    );
    let mut long = world.session.clone();
    long.title = "Fix the login redirect on Safari".to_owned();
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::SessionUpserted(long)));
    assert_eq!(areas(&world).list.width, 36);
    // What an agent is doing does not move it, nor resize the session.
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::SessionActivity {
            session: world.session.id,
            activity: Some(muster_core::Activity {
                text: "editing crates/muster-tui/src/view/sessions.rs".to_owned(),
                since: Timestamp::from_millis(0),
            }),
        }));
    assert_eq!(areas(&world).list.width, 36);
    // With no session beside them, the tickets take what the window
    // spares.
    press(&mut world.app, KeyCode::Char('3'));
    assert_eq!(world.app.view, View::Tickets);
    assert_eq!(areas(&world).list.width, 34);
}

#[test]
fn ctrl_backslash_folds_the_list_away_and_the_session_takes_its_place() {
    let mut world = shown();
    let actions = world.app.on_key(ctrl('\\'));
    let (req_id, change) = changed(&actions).expect("kept");
    assert_eq!(change, PreferenceChange::ListFolded(true));
    assert_eq!(
        resized(&actions),
        Some(TermSize {
            cols: 98,
            rows: PANE.rows
        })
    );
    let folded = areas(&world);
    assert!(folded.list.is_empty());
    assert_eq!((folded.preview.x, folded.preview.width), (0, 100));
    // The daemon's settings say so, then it answers.
    preferences(&mut world.app, |preferences| preferences.list_folded = true);
    done(&mut world, req_id);
    assert!(world.app.list_folded());

    let actions = world.app.on_key(ctrl('\\'));
    assert_eq!(
        changed(&actions).map(|(_, change)| change),
        Some(PreferenceChange::ListFolded(false))
    );
    assert_eq!(resized(&actions), Some(PANE));
    assert_eq!(areas(&world).list.width, 30);
}

#[test]
fn the_changes_sit_beside_the_session_where_the_window_has_room() {
    let mut world = shown();
    world.app.on_resize(WIDE);
    let id = world.session.id;
    press(&mut world.app, KeyCode::Char('d'));
    assert_eq!(world.app.mode, Mode::Changes);
    let wide = areas(&world);
    assert!(!wide.list.is_empty() && !wide.preview.is_empty());
    assert!(
        wide.changes.x > wide.preview.x,
        "beside the session: {wide:?}"
    );
    assert_eq!(
        world.app.attached,
        Some(id),
        "the session is still streamed"
    );

    // Too narrow for the three: the list and the changes, which have the
    // keys, open over the session, which is not streamed.
    let actions = world.app.on_resize(TermSize {
        cols: 100,
        rows: 29,
    });
    let narrower = areas(&world);
    assert!(narrower.preview.is_empty() && !narrower.changes.is_empty());
    assert!(actions.contains(&Action::Send(ClientFrame::Detach { session: id })));
}

#[test]
fn a_panel_dragged_by_its_title_goes_where_it_is_dropped_and_is_kept() {
    let mut world = shown();
    left(&mut world, MouseEventKind::Down, (5, 1));
    left(&mut world, MouseEventKind::Drag, (99, 12));
    assert!(matches!(
        world.app.panels.drag,
        Some(Drag::Panel {
            to: Some(Place::Window(Edge::Right)),
            ..
        })
    ));
    let screen = super::tests::render(&world.app);
    assert!(screen.contains(" here "), "where it would land: {screen}");
    let actions = left(&mut world, MouseEventKind::Up, (99, 12));
    let (_, change) = changed(&actions).expect("kept");
    let PreferenceChange::Panels(layout) = change else {
        panic!("{change:?}");
    };
    assert_eq!(
        layout.to_string(),
        "(session 60% | (changes / plugin)) | list"
    );
    let moved = areas(&world);
    assert_eq!((moved.list.x, moved.preview.x), (70, 0));
    assert_eq!(world.app.panels.drag, None);
}

#[test]
fn a_press_on_a_title_without_moving_gives_that_panel_the_keys() {
    let mut world = shown();
    let id = world.session.id;
    left(&mut world, MouseEventKind::Down, (40, 1));
    left(&mut world, MouseEventKind::Up, (40, 1));
    assert_eq!(world.app.mode, Mode::Terminal(id));
    left(&mut world, MouseEventKind::Down, (5, 1));
    let actions = left(&mut world, MouseEventKind::Up, (5, 1));
    assert_eq!(world.app.mode, Mode::Browse);
    assert_eq!(changed(&actions), None, "nothing moved");
}

#[test]
fn the_line_between_two_panels_shares_out_their_place_the_program_resized_as_it_is_let_go() {
    let mut world = shown();
    left(&mut world, MouseEventKind::Down, (29, 10));
    let dragging = left(&mut world, MouseEventKind::Drag, (39, 10));
    assert_eq!(resized(&dragging), None, "not resized while it moves");
    assert_eq!(areas(&world).list.width, 40);
    let actions = left(&mut world, MouseEventKind::Up, (39, 10));
    assert_eq!(
        resized(&actions),
        Some(TermSize {
            cols: 58,
            rows: PANE.rows
        })
    );
    let (_, change) = changed(&actions).expect("kept");
    assert_eq!(
        change,
        PreferenceChange::Panels(Layout::parse("list 40 | session 60% | changes").unwrap())
    );
}

#[test]
fn a_key_lets_go_of_a_drag_the_layout_as_it_was() {
    let mut world = shown();
    left(&mut world, MouseEventKind::Down, (29, 10));
    left(&mut world, MouseEventKind::Drag, (39, 10));
    assert_eq!(areas(&world).list.width, 40);
    let actions = press(&mut world.app, KeyCode::Esc);
    assert_eq!(world.app.panels.drag, None);
    assert_eq!(areas(&world).list.width, 30);
    assert_eq!(changed(&actions), None);
}

#[test]
fn m_moves_the_panels_with_the_keys_and_esc_puts_them_back() {
    let mut world = shown();
    press(&mut world.app, KeyCode::Char('m'));
    assert!(world.app.panels.arranging.is_some());
    let screen = super::tests::render(&world.app);
    assert!(screen.contains(" moving "), "{screen}");
    // Past its neighbour, the session.
    press(&mut world.app, KeyCode::Right);
    let moved = areas(&world);
    assert!(moved.list.x > moved.preview.x, "{moved:?}");
    let actions = press(&mut world.app, KeyCode::Esc);
    assert_eq!(changed(&actions), None);
    assert_eq!(world.app.panels.arranging, None);
    assert_eq!(*world.app.panels(), Layout::default());

    // Wider, and kept on ⏎.
    press(&mut world.app, KeyCode::Char('m'));
    press(&mut world.app, KeyCode::Char('+'));
    assert_eq!(areas(&world).list.width, 32);
    let actions = press(&mut world.app, KeyCode::Enter);
    assert_eq!(
        changed(&actions).map(|(_, change)| change),
        Some(PreferenceChange::Panels(
            Layout::parse("list 32 | session 60% | changes").unwrap()
        ))
    );
    assert_eq!(world.app.panels.arranging, None);
}

#[test]
fn the_settings_screen_puts_the_panels_back_as_they_were_at_first() {
    let mut world = shown();
    preferences(&mut world.app, |preferences| {
        preferences.panels = Layout::parse("session | list 40 | changes").unwrap();
        preferences.list_folded = true;
    });
    press(&mut world.app, KeyCode::Char('4'));
    world.app.settings.selected = Some(SettingRow::Panels);
    let screen = super::tests::render(&world.app);
    assert!(screen.contains("moved, list folded"), "{screen}");
    let actions = press(&mut world.app, KeyCode::Enter);
    let changes: Vec<PreferenceChange> = actions
        .iter()
        .filter_map(|action| changed(std::slice::from_ref(action)).map(|(_, change)| change))
        .collect();
    assert_eq!(
        changes,
        [
            PreferenceChange::ListFolded(false),
            PreferenceChange::Panels(Layout::default())
        ]
    );
}

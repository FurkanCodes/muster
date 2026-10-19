//! A window too narrow for both shows the list or the session, not both
//! (`PRODUCT.md` §6 Sessions): from how many columns they sit side by
//! side is Muster's own setting, on the Settings screen.

use crossterm::event::KeyCode;
use muster_core::protocol::{ClientFrame, Command, Delta, Reply, ServerFrame, TermSize};
use muster_core::settings::{PreferenceChange, Preferences};
use ratatui::layout::Rect;

use super::tests::{World, press, render_in, shift_q, world};
use super::{Action, Mode, SettingRow, View};
use crate::model::Row;

const NARROW: TermSize = TermSize { cols: 90, rows: 29 };

fn window(size: TermSize) -> Rect {
    Rect::new(0, 0, size.cols, size.rows + 1)
}

/// The world in a window `size` big, the session selected.
fn sized(size: TermSize) -> World {
    let mut world = world();
    world.app.selected = Some(Row::Session(world.session.id));
    world.app.on_resize(size);
    world
}

/// The change to Muster's own settings `actions` ask for, if any.
fn asked(actions: &[Action]) -> Option<(u64, PreferenceChange)> {
    actions.iter().find_map(|action| match action {
        Action::Send(ClientFrame::Command {
            req_id,
            command: Command::ChangePreference(change),
        }) => Some((*req_id, change.clone())),
        _ => None,
    })
}

fn side_by_side_from(world: &mut World, columns: u16) {
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::Preferences(Preferences {
            side_by_side_from: columns,
            ..Preferences::default()
        })));
}

#[test]
fn below_a_hundred_columns_the_list_takes_the_window_and_streams_no_session() {
    let mut world = sized(NARROW);
    assert!(world.app.narrow());
    let areas = crate::view::browse_areas(&world.app, window(NARROW));
    assert_eq!(areas.list.width, NARROW.cols);
    assert_eq!(areas.preview.width, 0);
    let screen = render_in(&world.app, NARROW);
    assert!(!screen.contains("╭ Shell"), "no pane beside it: {screen}");
    assert!(screen.contains("○ ❯ Shell"), "{screen}");
    // Selecting a session streams nothing: nobody would see it.
    press(&mut world.app, KeyCode::Up);
    let actions = press(&mut world.app, KeyCode::Down);
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Send(ClientFrame::Attach { .. }))),
        "{actions:?}"
    );
    // A hundred columns are enough for both.
    world.app.on_resize(super::tests::SIZE);
    assert!(!world.app.narrow());
}

#[test]
fn enter_opens_the_session_over_the_list_at_its_size_and_shift_q_goes_back() {
    let mut world = sized(NARROW);
    let session = world.session.id;
    let actions = press(&mut world.app, KeyCode::Enter);
    assert_eq!(world.app.mode, Mode::Terminal(session));
    assert!(world.app.fills_window());
    assert!(
        !world.app.full_screen,
        "the person did not ask for full screen"
    );
    assert!(
        actions.contains(&Action::Send(ClientFrame::Resize {
            session,
            size: NARROW,
            pixels: None,
        })),
        "its program draws for the whole window: {actions:?}"
    );
    assert!(
        actions
            .iter()
            .any(|action| matches!(action, Action::Send(ClientFrame::Attach { .. })))
    );
    let screen = render_in(&world.app, NARROW);
    assert!(!screen.contains(" projects "), "{screen}");

    let actions = world.app.on_key(shift_q());
    assert_eq!(world.app.mode, Mode::Browse);
    assert!(actions.contains(&Action::Send(ClientFrame::Detach { session })));
    assert!(render_in(&world.app, NARROW).contains(" projects "));
}

#[test]
fn a_window_widened_or_narrowed_resizes_the_session_to_where_it_is_drawn() {
    let mut world = sized(NARROW);
    let session = world.session.id;
    press(&mut world.app, KeyCode::Enter);
    let wide = TermSize {
        cols: 120,
        rows: 29,
    };
    let actions = world.app.on_resize(wide);
    let pane = crate::view::session_area(&world.app, window(wide));
    let beside = TermSize {
        cols: pane.width,
        rows: pane.height,
    };
    assert_eq!(
        actions,
        [Action::Send(ClientFrame::Resize {
            session,
            size: beside,
            pixels: None,
        })]
    );
    assert!(!world.app.fills_window(), "beside the list again");
    let actions = world.app.on_resize(NARROW);
    assert_eq!(
        actions,
        [Action::Send(ClientFrame::Resize {
            session,
            size: NARROW,
            pixels: None,
        })]
    );
}

#[test]
fn the_changes_take_the_whole_width_while_they_have_the_keys() {
    let mut world = sized(NARROW);
    world.app.selected = Some(Row::Worktree(world.worktree.id));
    press(&mut world.app, KeyCode::Char('d'));
    assert_eq!(world.app.mode, Mode::Changes);
    let areas = crate::view::browse_areas(&world.app, window(NARROW));
    assert_eq!(areas.list.width, 0);
    assert_eq!(areas.changes.width, NARROW.cols);
    press(&mut world.app, KeyCode::Char('d'));
    let areas = crate::view::browse_areas(&world.app, window(NARROW));
    assert_eq!(areas.list.width, NARROW.cols);
}

#[test]
fn set_to_any_width_both_are_side_by_side_however_narrow() {
    let mut world = sized(TermSize { cols: 60, rows: 29 });
    assert!(world.app.narrow());
    side_by_side_from(&mut world, 0);
    assert!(!world.app.narrow());
    side_by_side_from(&mut world, 120);
    world.app.on_resize(super::tests::SIZE);
    assert!(world.app.narrow(), "a hundred columns are too few for 120");
}

#[test]
fn the_width_is_one_of_muster_s_own_settings() {
    let mut world = world();
    press(&mut world.app, KeyCode::Char('4'));
    assert_eq!(world.app.view, View::Settings);
    assert!(world.app.setting_rows().ends_with(&[
        SettingRow::Muster,
        SettingRow::SideBySide,
        SettingRow::Colours,
        SettingRow::SessionLines,
        SettingRow::Panels,
        SettingRow::NamesByAgent,
        SettingRow::NamedBy,
        SettingRow::NamingModel,
        SettingRow::Agents
    ]));
    world.app.settings_selected = Some(SettingRow::SideBySide);
    let screen = super::tests::render(&world.app);
    assert!(screen.contains("Session beside the list from"), "{screen}");
    assert!(screen.contains("100 columns"), "{screen}");
    assert!(screen.contains("⏎ next width"), "{screen}");

    // ⏎ goes to the next width; the answer says so, and the state has it.
    let (req_id, change) = asked(&press(&mut world.app, KeyCode::Enter)).unwrap();
    assert_eq!(change, PreferenceChange::SideBySideFrom(110));
    let screen = super::tests::render(&world.app);
    let row = screen
        .lines()
        .find(|line| line.contains("Session beside the list from"))
        .unwrap();
    assert!(row.contains('…') && !row.contains("columns"), "{row}");
    assert_eq!(
        asked(&press(&mut world.app, KeyCode::Enter)),
        None,
        "one at a time"
    );
    side_by_side_from(&mut world, 110);
    world.app.on_frame(ServerFrame::Reply {
        req_id,
        outcome: Ok(Reply::Done),
    });
    let screen = super::tests::render(&world.app);
    assert!(screen.contains("110 columns"), "{screen}");
    assert!(screen.contains("window of 110 columns or more"), "{screen}");

    // ← and → go to a smaller or a larger one, no further than either end.
    let change = asked(&press(&mut world.app, KeyCode::Left)).unwrap().1;
    assert_eq!(change, PreferenceChange::SideBySideFrom(100));
}

#[test]
fn from_the_widest_the_next_width_is_any() {
    let mut world = world();
    side_by_side_from(&mut world, 160);
    world.app.view = View::Settings;
    world.app.settings_selected = Some(SettingRow::SideBySide);
    assert_eq!(
        asked(&press(&mut world.app, KeyCode::Right)),
        None,
        "the widest"
    );
    let change = asked(&press(&mut world.app, KeyCode::Enter)).unwrap().1;
    assert_eq!(change, PreferenceChange::SideBySideFrom(0));
}

#[test]
fn a_daemon_that_does_not_keep_the_width_is_not_asked_to_change_it() {
    let mut world = world();
    world.app.daemon_protocol = muster_core::settings::SIDE_BY_SIDE_PROTOCOL - 1;
    assert!(!world.app.setting_rows().contains(&SettingRow::SideBySide));
    // The window still follows the width it has, by default.
    world.app.on_resize(NARROW);
    assert!(world.app.narrow());
}

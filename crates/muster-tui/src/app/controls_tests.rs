//! What a click presses (`ARC-32`, `PRODUCT.md` §6): a hint presses the
//! key it names; a tab shows its screen, a count its queue, ZOOM puts the
//! zoomed panel back; the list's fold button folds it and brings it back;
//! a double-click on a line does what ⏎ does, on a title zooms; and a
//! right-click opens a menu on every screen's lines and the list's empty
//! part.

use crossterm::event::{MouseButton, MouseEventKind};
use muster_core::layout::{Edge, Panel, Place};
use muster_core::protocol::{Delta, ServerFrame, TermSize};
use ratatui::layout::{Position, Rect};

use super::controls::Control;
use super::pointer_tests::mouse;
use super::tests::{World, render_in, world};
use super::{Mode, View};
use crate::model::Row;
use crate::model::tests::session;
use crate::view::top_bar::TopControl;

const WIDE: TermSize = TermSize {
    cols: 200,
    rows: 30,
};

fn wide() -> World {
    let mut world = world();
    world.app.on_resize(WIDE);
    world
}

fn areas(world: &World) -> crate::view::BrowseAreas {
    let size = world.app.terminal_size;
    crate::view::browse_areas(&world.app, Rect::new(0, 0, size.cols, size.rows + 1))
}

/// Where `wanted` is on the screen, its first cell.
fn control(world: &World, wanted: Control) -> Position {
    world
        .app
        .controls()
        .into_iter()
        .find(|(control, _)| *control == wanted)
        .map_or_else(
            || panic!("no {wanted:?} in {:?}", world.app.controls()),
            |(_, cells)| Position::new(cells.x, cells.y),
        )
}

/// A press and its release at `at`, `ms` into the clock.
fn click_at(world: &mut World, at: Position, ms: u64) {
    world.app.on_mouse_at(
        mouse(MouseEventKind::Down(MouseButton::Left), at.x, at.y),
        ms,
    );
    world
        .app
        .on_mouse_at(mouse(MouseEventKind::Up(MouseButton::Left), at.x, at.y), ms);
}

#[test]
fn a_hint_presses_the_key_it_names() {
    let mut world = wide();
    world.app.selected = Some(Row::Session(world.session.id));
    world.app.settle();
    // `f full screen`, among the session's hints.
    let full = crate::keys::labelled("f").unwrap();
    let at = control(&world, Control::Hint(full));
    click_at(&mut world, at, 0);
    assert_eq!(world.app.mode, Mode::Terminal(world.session.id));
    assert!(world.app.full_screen);
}

#[test]
fn a_tab_shows_its_screen_and_a_count_its_queue() {
    let mut world = wide();
    let at = control(&world, Control::Top(TopControl::Tab(View::Tickets)));
    click_at(&mut world, at, 0);
    assert_eq!(world.app.view, View::Tickets);
    let mut asking = session(world.worktree.id, "Asking", 5);
    asking.status = muster_core::SessionStatus::NeedsYou {
        reason: muster_core::WaitReason::Question,
    };
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::SessionUpserted(asking)));
    // The count of those waiting, after the tabs: past the Needs you tab.
    let tabs = world.app.controls();
    let (_, count) = tabs
        .iter()
        .filter(|(control, _)| *control == Control::Top(TopControl::Tab(View::NeedsYou)))
        .nth(1)
        .copied()
        .unwrap_or_else(|| panic!("the count: {tabs:?}"));
    click_at(&mut world, Position::new(count.x, count.y), 0);
    assert_eq!(world.app.view, View::NeedsYou);
}

#[test]
fn the_fold_button_folds_the_list_and_brings_it_back() {
    let mut world = wide();
    let at = control(&world, Control::Fold);
    assert!(render_in(&world.app, WIDE).contains(" ‹ "));
    click_at(&mut world, at, 0);
    assert!(world.app.list_folded());
    assert!(areas(&world).list.is_empty());
    // At the window's edge now.
    let at = control(&world, Control::Fold);
    assert_eq!(at.x, 0);
    click_at(&mut world, at, 0);
    assert!(!world.app.list_folded());
}

#[test]
fn zoom_in_the_top_bar_puts_the_zoomed_panel_back() {
    let mut world = wide();
    world.app.selected = Some(Row::Session(world.session.id));
    world.app.settle();
    world.app.panels.zoomed = Some(Panel::Session);
    let at = control(&world, Control::Top(TopControl::Zoom));
    click_at(&mut world, at, 0);
    assert_eq!(world.app.panels.zoomed, None);
}

#[test]
fn a_double_click_on_a_line_does_what_enter_does_and_on_a_title_zooms() {
    let mut world = wide();
    let list = areas(&world).list;
    let rows = world.app.rows();
    let at = rows
        .iter()
        .position(|row| *row == Row::Session(world.session.id))
        .unwrap();
    let heights = world.app.row_heights(&rows);
    let line: usize = heights[..at].iter().sum();
    let on = Position::new(list.x + 8, list.y + 1 + u16::try_from(line).unwrap());
    click_at(&mut world, on, 1_000);
    assert_eq!(world.app.mode, Mode::Browse, "one click selects");
    click_at(&mut world, on, 1_200);
    assert_eq!(world.app.mode, Mode::Terminal(world.session.id), "two open");
    // Too slow, two clicks are two clicks.
    world.app.mode = Mode::Browse;
    click_at(&mut world, on, 5_000);
    click_at(&mut world, on, 5_900);
    assert_eq!(world.app.mode, Mode::Browse);

    let preview = areas(&world).preview;
    let title = Position::new(preview.x + 3, preview.y);
    click_at(&mut world, title, 9_000);
    click_at(&mut world, title, 9_100);
    assert_eq!(world.app.panels.zoomed, Some(Panel::Session));
    click_at(&mut world, title, 12_000);
    click_at(&mut world, title, 12_100);
    assert_eq!(world.app.panels.zoomed, None);
}

#[test]
fn a_right_click_on_the_list_s_empty_part_offers_what_starts_something() {
    let mut world = wide();
    let list = areas(&world).list;
    world.app.on_mouse(mouse(
        MouseEventKind::Down(MouseButton::Right),
        list.x + 4,
        list.bottom() - 3,
    ));
    let Mode::Menu(menu) = world.app.mode.clone() else {
        panic!("{:?}", world.app.mode);
    };
    let whats: Vec<&str> = menu.items.iter().map(|item| item.what.as_str()).collect();
    assert_eq!(
        whats,
        [
            "add a project",
            "clone one from GitHub",
            "the session told of last",
            "every key"
        ]
    );
}

#[test]
fn a_right_click_on_a_ticket_or_a_setting_offers_what_its_keys_do() {
    let mut world = super::tickets_tests::on_tickets();
    world.app.on_resize(WIDE);
    let list = areas(&world).list;
    // #42, the second line.
    world.app.on_mouse(mouse(
        MouseEventKind::Down(MouseButton::Right),
        list.x + 6,
        list.y + 2,
    ));
    let Mode::Menu(menu) = world.app.mode.clone() else {
        panic!("{:?}", world.app.mode);
    };
    let items: Vec<String> = menu
        .items
        .iter()
        .map(|item| format!("{} {}", item.key, item.what))
        .collect();
    assert!(items.contains(&"⏎ start an agent".to_owned()), "{items:?}");
    assert!(items.contains(&"x close".to_owned()), "{items:?}");

    let mut world = super::settings_tests::on_settings(true);
    world.app.on_resize(WIDE);
    let list = areas(&world).list;
    world.app.on_mouse(mouse(
        MouseEventKind::Down(MouseButton::Right),
        list.x + 6,
        list.y + 2,
    ));
    assert!(
        matches!(world.app.mode, Mode::Menu(_)),
        "{:?}",
        world.app.mode
    );
}

#[test]
fn a_title_s_button_is_lit_under_the_pointer() {
    let mut world = wide();
    let second = session(world.worktree.id, "Claude", 4);
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::SessionUpserted(second.clone())));
    world.app.selected = Some(Row::Session(world.session.id));
    world.app.settle();
    world
        .app
        .place_at(second.id, Place::Beside(Panel::Session, Edge::Right));
    let at = control(
        &world,
        Control::Button(Panel::Placed(second.id), crate::view::pane::Button::TakeOff),
    );
    assert_eq!(world.app.lit_control(), None);
    world
        .app
        .on_mouse(mouse(MouseEventKind::Moved, at.x + 1, at.y));
    assert!(
        world
            .app
            .lit_control()
            .is_some_and(|cells| cells.contains(at))
    );
}

#[test]
fn a_click_on_a_setting_s_value_changes_it_and_its_arrows_step_it() {
    let mut world = super::settings_tests::on_settings(true);
    world.app.on_resize(WIDE);
    let list = areas(&world).list;
    let rows = world.app.setting_rows();
    let width = usize::from(list.width.saturating_sub(2));
    let line_of = |row| {
        let at = rows.iter().position(|each| *each == row).unwrap();
        list.y + 1 + u16::try_from(at).unwrap()
    };
    let value = |world: &World, row| crate::view::settings::value_keys(&world.app, row, width);
    // Chats named by an agent: a click on its switch turns it off.
    let notch = super::SettingRow::NamesByAgent;
    let (from, _, key) = value(&world, notch)[0];
    assert_eq!(key, crossterm::event::KeyCode::Enter);
    let actions = world.app.on_mouse(mouse(
        MouseEventKind::Down(MouseButton::Left),
        list.x + 1 + from,
        line_of(notch),
    ));
    assert!(
        matches!(
            super::tests::command(&actions).1,
            muster_core::protocol::Command::ChangePreference(
                muster_core::settings::PreferenceChange::NamesByAgent(false)
            )
        ),
        "{actions:?}"
    );
    // The width a session sits beside the list from: its arrows.
    let side = super::SettingRow::SideBySide;
    let keys = value(&world, side);
    assert_eq!(
        keys.iter().map(|(_, _, key)| *key).collect::<Vec<_>>(),
        [
            crossterm::event::KeyCode::Left,
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyCode::Right
        ]
    );
    assert!(render_in(&world.app, WIDE).contains("‹ 100 columns ›"));
    let (from, _, _) = keys[2];
    let actions = world.app.on_mouse(mouse(
        MouseEventKind::Down(MouseButton::Left),
        list.x + 1 + from,
        line_of(side),
    ));
    assert!(
        matches!(
            super::tests::command(&actions).1,
            muster_core::protocol::Command::ChangePreference(
                muster_core::settings::PreferenceChange::SideBySideFrom(110)
            )
        ),
        "{actions:?}"
    );
}

#[test]
fn a_click_on_a_ticket_part_s_box_gives_it_to_the_agent_or_not() {
    let mut world = super::tickets_tests::on_tickets();
    world.app.on_resize(WIDE);
    let ticket = (world.project.id, super::tickets_tests::key(42));
    world.app.tickets.texts.insert(
        ticket.clone(),
        crate::app::TicketText::Loaded {
            body: "It fails.".to_owned(),
            comments: vec![muster_core::IssueComment {
                author: Some("mira".to_owned()),
                at: None,
                body: "Fixed in #43.".to_owned(),
            }],
        },
    );
    let screen = render_in(&world.app, WIDE);
    let row = screen
        .lines()
        .position(|line| line.contains("☐ mira"))
        .unwrap_or_else(|| panic!("{screen}"));
    let preview = areas(&world).preview;
    world.app.on_mouse(mouse(
        MouseEventKind::Down(MouseButton::Left),
        preview.x + 3,
        u16::try_from(row).unwrap(),
    ));
    let brief = world.app.ticket_brief(ticket.0, &ticket.1);
    assert_eq!(brief.comments.len(), 1, "mira's given: {brief:?}");
    assert!(render_in(&world.app, WIDE).contains("☑ mira"));
}

#[test]
fn a_project_dragged_onto_another_goes_there_and_settings_put_them_back() {
    let mut world = wide();
    let other = crate::model::tests::project("web");
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::ProjectUpserted(other.clone())));
    let first = world.app.project_line_area(world.project.id).unwrap();
    let second = world.app.project_line_area(other.id).unwrap();
    world.app.on_mouse(mouse(
        MouseEventKind::Down(MouseButton::Left),
        first.x + 6,
        first.y,
    ));
    world.app.on_mouse(mouse(
        MouseEventKind::Drag(MouseButton::Left),
        second.x + 6,
        second.y,
    ));
    let actions = world.app.on_mouse(mouse(
        MouseEventKind::Up(MouseButton::Left),
        second.x + 6,
        second.y,
    ));
    let (_, asked) = super::tests::command(&actions);
    assert_eq!(
        asked,
        muster_core::protocol::Command::ChangePreference(
            muster_core::settings::PreferenceChange::ProjectOrder(vec![other.id, world.project.id])
        )
    );
    // Kept, as the daemon says: the list follows.
    let mut preferences = world.app.model.preferences().clone();
    preferences.projects = vec![other.id, world.project.id];
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::Preferences(preferences)));
    assert_eq!(world.app.rows().first(), Some(&Row::Project(other.id)));
    // Settings › Projects' order puts them back.
    world.app.show(View::Settings);
    world.app.settings.selected = Some(super::SettingRow::ProjectOrder);
    let actions = super::tests::press(&mut world.app, crossterm::event::KeyCode::Enter);
    assert_eq!(
        super::tests::command(&actions).1,
        muster_core::protocol::Command::ChangePreference(
            muster_core::settings::PreferenceChange::ProjectOrder(Vec::new())
        )
    );
}

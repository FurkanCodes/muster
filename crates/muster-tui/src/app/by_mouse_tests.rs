//! Whatever a key does, the mouse does too (`ARC-32`, `PRODUCT.md` §6):
//! every action in the key table is reached on some screen by a click on
//! a hint, an item of a right-click menu or a button, collected here from
//! the screens as they are, never from a list kept by hand; or it is one
//! the mouse does another way, each said below with how. A key added
//! without its mouse fails here, named.

use std::collections::BTreeSet;

use crossterm::event::KeyCode;
use muster_core::keys::{Action as Does, Screen};
use muster_core::layout::{Edge, Panel, Place};
use muster_core::protocol::{Delta, ServerFrame, TermSize};
use muster_core::{SessionStatus, Timestamp};

use super::tests::{World, ctrl, world};
use super::{App, Mode, View};
use crate::app::controls::Control;
use crate::model::Row;
use crate::model::tests::session;
use crate::view::pane::Button;
use crate::view::top_bar::TopControl;

/// What the mouse does instead of an action's key, and how.
const OTHERWISE: &[(Does, &str)] = &[
    (
        Does::MoveUp,
        "a click selects a line, and the wheel moves the selection",
    ),
    (
        Does::MoveDown,
        "a click selects a line, and the wheel moves the selection",
    ),
    (
        Does::MovePanels,
        "a panel is dragged by its title, a line between two to size them",
    ),
    (
        Does::LeaveTab,
        "a terminal tab of the person's own has no Muster window to click",
    ),
    (Does::FocusLeft, "a click on a panel gives it the keys"),
    (Does::FocusDown, "a click on a panel gives it the keys"),
    (Does::FocusUp, "a click on a panel gives it the keys"),
    (Does::FocusRight, "a click on a panel gives it the keys"),
    (Does::ToPanel, "a click on a panel gives it the keys"),
    (Does::ToList, "a click on the list gives it the keys back"),
    (
        Does::SendPanelKey,
        "the mouse never took the panel key, so the program has it as typed",
    ),
    (Does::NextSpace, "the spaces menu goes to any space"),
    (Does::PreviousSpace, "the spaces menu goes to any space"),
];

/// The screen whose keys a press of a hint's key is, as the app is.
fn screen(app: &App) -> Option<Screen> {
    if app.panels.waiting.is_some() {
        return Some(Screen::Prefix);
    }
    match app.mode {
        Mode::Terminal(_) => Some(Screen::InSession),
        Mode::Changes => Some(Screen::Changes),
        Mode::Browse => Some(match app.view {
            View::Sessions => Screen::Sessions,
            View::NeedsYou => Screen::NeedsYou,
            View::Tickets => Screen::Tickets,
            View::Settings => Screen::Settings,
        }),
        _ => None,
    }
}

/// What a key `label` names does on `app`'s screen.
fn named(app: &App, label: &str) -> Option<Does> {
    let key = crate::keys::labelled(label)?;
    let chord = crate::keys::chord(&key)?;
    app.keymap.action(screen(app)?, chord)
}

/// Every action the mouse reaches on the screen `world` shows, into
/// `reached`: its hints, tabs, counts and buttons, and the menus a
/// right-click on its lines, its empty list and its panels' titles open.
fn reach(world: &World, reached: &mut BTreeSet<Does>) {
    let app = &world.app;
    for (control, _) in app.controls() {
        match control {
            Control::Hint(key) => {
                let does = crate::keys::chord(&key)
                    .zip(screen(app))
                    .and_then(|(chord, screen)| app.keymap.action(screen, chord));
                reached.extend(does);
            }
            // Any screen is a click away: the next, the one before, each.
            Control::Top(TopControl::Tab(view)) => reached.extend([
                Does::NextTab,
                Does::PreviousTab,
                match view {
                    View::Sessions => Does::SessionsTab,
                    View::NeedsYou => Does::NeedsYouTab,
                    View::Tickets => Does::TicketsTab,
                    View::Settings => Does::SettingsTab,
                },
            ]),
            Control::Top(TopControl::Zoom) => {
                reached.insert(Does::Zoom);
            }
            Control::Button(_, Button::TakeOff) => {
                reached.insert(Does::TakeOff);
            }
            Control::Button(_, Button::Place) => {
                reached.insert(Does::PlacePreview);
            }
            Control::Fold => {
                reached.insert(Does::FoldList);
            }
        }
    }
    // The menus: on the selected line, on the list's empty part, and on
    // each panel's title.
    for item in app.actions_here() {
        reached.extend(item.does);
    }
    for hint in crate::view::line_hints(app) {
        if let Some(item) = super::Here::of_hint(&hint) {
            reached.extend(named(app, &item.key));
        }
    }
    for (panel, _) in app.shown_panels() {
        for item in app.title_items(panel) {
            reached.extend(item.does);
        }
    }
    if matches!(app.view, View::Sessions | View::NeedsYou) {
        for item in app.empty_list_items() {
            reached.extend(item.does);
        }
    }
}

/// Each line of the sessions list selected in turn, what the mouse
/// reaches there.
fn each_line(world: &mut World, reached: &mut BTreeSet<Does>) {
    for row in world.app.rows() {
        world.app.selected = Some(row);
        world.app.settle();
        reach(world, reached);
    }
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "one walk through every screen, each step reaching what it shows"
)]
fn every_key_has_a_mouse_arc_32() {
    let mut reached = BTreeSet::new();
    let wide = TermSize {
        cols: 200,
        rows: 40,
    };

    // The sessions list: a project, a worktree with a pull request, a
    // session at work, one ended; each line in turn.
    let mut world = world();
    world.app.on_resize(wide);
    let mut worktree = world.worktree.clone();
    worktree.pull_request = Some(super::pull_request_tests::approved());
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::WorkspaceUpserted(worktree)));
    let mut ended = session(world.worktree.id, "Ended", 7);
    ended.status = SessionStatus::Exited { success: true };
    ended.ended_at = Some(Timestamp::from_millis(8));
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::SessionUpserted(ended)));
    // And a terminal in no project, under its space.
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::SessionUpserted(
            crate::model::tests::free_terminal("zsh", 12),
        )));
    each_line(&mut world, &mut reached);

    // Typing in a session, two panels side by side, the panel key waiting.
    let mut second = session(world.worktree.id, "Claude", 9);
    second.status = SessionStatus::Running;
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::SessionUpserted(second.clone())));
    world.app.selected = Some(Row::Session(world.session.id));
    world.app.settle();
    world
        .app
        .place_at(second.id, Place::Beside(Panel::Session, Edge::Right));
    world.app.mode = Mode::Terminal(world.session.id);
    world.app.settle();
    reach(&world, &mut reached);
    world.app.on_key(ctrl('b'));
    reach(&world, &mut reached);
    world.app.on_key(super::tests::key(KeyCode::Char('z')));
    reach(&world, &mut reached);
    world.app.panels.zoomed = None;
    // And one above another.
    let mut third = session(world.worktree.id, "Codex", 11);
    third.status = SessionStatus::Running;
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::SessionUpserted(third.clone())));
    world.app.place_at(
        third.id,
        Place::Beside(Panel::Placed(second.id), Edge::Bottom),
    );
    world.app.settle();
    reach(&world, &mut reached);

    // The queue.
    let mut asking = session(world.worktree.id, "Asking", 10);
    asking.status = SessionStatus::NeedsYou {
        reason: muster_core::WaitReason::Question,
    };
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::SessionUpserted(asking)));
    world.app.mode = Mode::Browse;
    world.app.show(View::NeedsYou);
    world.app.settle();
    reach(&world, &mut reached);

    // A ticket with an agent on it, its text and what was said on it.
    let mut world = super::tickets_tests::on_tickets();
    super::tickets_tests::agent_on_42(&mut world, "muster/issue-42-fix", 20);
    world.app.tickets.texts.insert(
        (world.project.id, super::tickets_tests::key(42)),
        crate::app::TicketText::Loaded {
            body: "After signing in, users land on /404.".to_owned(),
            comments: vec![muster_core::IssueComment {
                author: Some("octocat".to_owned()),
                at: None,
                body: "Seen on main too.".to_owned(),
            }],
        },
    );
    world.app.settle();
    reach(&world, &mut reached);

    // The settings, and a plugin from git among them.
    let world = super::plugins_tests::on(crate::app::SettingRow::Plugin(
        crate::app::PluginKey::of("zoe").unwrap(),
    ));
    reach(&world, &mut reached);

    // A workspace's changes, a turn of its timeline chosen.
    let mut world = world_wide(wide);
    super::timeline::tests::opened(&mut world);
    reach(&world, &mut reached);

    let otherwise: BTreeSet<Does> = OTHERWISE.iter().map(|(does, _)| *does).collect();
    let without: Vec<&str> = Does::ALL
        .iter()
        .filter(|does| !reached.contains(does) && !otherwise.contains(does))
        .map(|does| does.name())
        .collect();
    assert!(
        without.is_empty(),
        "no click, menu or button does these: {without:?}"
    );
    // And none said to be done otherwise is reached by a click after all.
    let both: Vec<&str> = OTHERWISE
        .iter()
        .filter(|(does, _)| reached.contains(does))
        .map(|(does, _)| does.name())
        .collect();
    assert!(
        both.is_empty(),
        "said to be done otherwise, but clicked: {both:?}"
    );
}

fn world_wide(size: TermSize) -> World {
    let mut world = world();
    world.app.on_resize(size);
    world
}

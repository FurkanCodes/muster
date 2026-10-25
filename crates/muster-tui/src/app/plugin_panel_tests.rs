//! A plugin's panel beside the agent (`PRODUCT.md` §6, `PRD-15`): shown,
//! streamed, while one runs in the selected line's workspace, and only
//! then, so nothing moves without one; it follows the selection, a click
//! moves the keys to it and Shift-Q back, and one that ends frees its
//! place and the keys. It stands by the chats as a shell does: on one
//! line, never counted among the agents, never news.

use crossterm::event::{MouseButton, MouseEventKind};
use muster_core::protocol::{ClientFrame, Delta, ServerFrame, TermSize};
use muster_core::{Session, SessionId, SessionStatus, Timestamp};
use ratatui::layout::Rect;

use super::pointer_tests::mouse;
use super::tests::{World, render_in, shift_q, world};
use super::{Action, Mode};
use crate::model::Row;
use crate::model::tests::{session, worktree};

const WIDE: TermSize = TermSize {
    cols: 180,
    rows: 29,
};

fn areas(world: &World) -> crate::view::BrowseAreas {
    let size = world.app.terminal_size;
    crate::view::browse_areas(&world.app, Rect::new(0, 0, size.cols, size.rows + 1))
}

/// The world in a wide window, its session selected beside the list.
fn wide() -> World {
    let mut world = world();
    world.app.selected = Some(Row::Session(world.session.id));
    world.app.on_resize(WIDE);
    world
}

/// The state says `session` is so; what the app does about it.
fn upsert(world: &mut World, session: Session) -> Vec<Action> {
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::SessionUpserted(session)))
}

/// A plugin's panel running in the world's worktree.
fn a_panel(world: &mut World) -> (Session, Vec<Action>) {
    let mut panel = session(world.worktree.id, "graph", 5);
    panel.harness = "plugin:zoetrope/graph".to_owned();
    panel.status = SessionStatus::Running;
    let actions = upsert(world, panel.clone());
    (panel, actions)
}

fn sent(actions: &[Action], attach: bool) -> Vec<SessionId> {
    actions
        .iter()
        .filter_map(|action| match action {
            Action::Send(ClientFrame::Attach { session, .. }) if attach => Some(*session),
            Action::Send(ClientFrame::Detach { session }) if !attach => Some(*session),
            _ => None,
        })
        .collect()
}

fn click(world: &mut World, area: Rect) {
    world.app.on_mouse(mouse(
        MouseEventKind::Down(MouseButton::Left),
        area.x + 5,
        area.y + 3,
    ));
}

#[test]
fn without_a_plugin_s_panel_nothing_is_shown_for_one() {
    let mut world = wide();
    let before = areas(&world);
    assert!(before.plugin.is_empty());
    // A shell placed beside the session, as before.
    let mut shell = session(world.worktree.id, "zsh", 4);
    shell.harness = "shell".to_owned();
    upsert(&mut world, shell.clone());
    world.app.panels.kept = Some(world.app.panels().placing(
        shell.id,
        muster_core::layout::Place::Beside(
            muster_core::layout::Panel::Session,
            muster_core::layout::Edge::Right,
        ),
    ));
    world.app.settle();
    let with_shell = areas(&world);
    assert!(
        with_shell.plugin.is_empty()
            && !with_shell
                .of(muster_core::layout::Panel::Placed(shell.id))
                .is_empty()
    );
    assert_eq!(world.app.plugin_stream.session, None);
}

#[test]
fn a_plugin_s_panel_in_the_workspace_is_streamed_beside_the_session() {
    let mut world = wide();
    let (panel, actions) = a_panel(&mut world);
    assert_eq!(sent(&actions, true), [panel.id]);
    assert_eq!(world.app.plugin_stream.session, Some(panel.id));
    assert_eq!(world.app.attached, Some(world.session.id), "the agent too");
    let shown = areas(&world);
    assert!(!shown.preview.is_empty() && !shown.plugin.is_empty());
    assert!(shown.plugin.x > shown.preview.x, "beside the session");
    let screen = render_in(&world.app, WIDE);
    assert!(screen.contains("graph"), "{screen}");
    // Its program is given the size of its panel.
    let inside = world.app.plugin_size();
    assert_eq!(inside.cols, shown.plugin.width - 2);
}

#[test]
fn the_panel_follows_the_selection_and_its_stream_goes_with_it() {
    let mut world = wide();
    let (panel, _) = a_panel(&mut world);
    // Another worktree, with no panel: none beside its line.
    let other = worktree(world.project.id, "other");
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::WorkspaceUpserted(other.clone())));
    world.app.selected = Some(Row::Worktree(other.id));
    let actions = world.app.settle();
    assert!(sent(&actions, false).contains(&panel.id), "{actions:?}");
    assert_eq!(world.app.plugin_beside(), None);
    assert!(areas(&world).plugin.is_empty());
    // Back on the panel's worktree: beside it again.
    world.app.selected = Some(Row::Worktree(world.worktree.id));
    let actions = world.app.settle();
    assert_eq!(sent(&actions, true), [panel.id]);
    assert_eq!(world.app.plugin_beside(), Some(panel.id));
    // Its own line selected, the session panel shows it instead.
    world.app.selected = Some(Row::Session(panel.id));
    let actions = world.app.settle();
    assert!(
        sent(&actions, true).is_empty() && sent(&actions, false).is_empty(),
        "{actions:?}"
    );
    assert_eq!(world.app.attached, Some(panel.id));
    assert_eq!(world.app.plugin_beside(), None);
}

#[test]
fn a_click_gives_the_panel_the_keys_and_shift_q_or_a_click_takes_them_back() {
    let mut world = wide();
    let (panel, _) = a_panel(&mut world);
    let area = areas(&world).plugin;
    click(&mut world, area);
    assert_eq!(world.app.mode, Mode::Terminal(panel.id));
    assert!(world.app.typing_in_plugin());
    assert_eq!(
        world.app.focused_session(),
        Some(world.session.id),
        "still the one selected"
    );
    assert_eq!(world.app.focus_session(), Some(panel.id));
    world.app.on_key(shift_q());
    assert_eq!(world.app.mode, Mode::Browse);
    // From the panel to the session beside it, and on to the shell.
    let area = areas(&world).plugin;
    click(&mut world, area);
    let area = areas(&world).preview;
    click(&mut world, area);
    assert_eq!(world.app.mode, Mode::Terminal(world.session.id));
    let area = areas(&world).plugin;
    click(&mut world, area);
    assert_eq!(world.app.mode, Mode::Terminal(panel.id));
}

#[test]
fn a_panel_that_ends_frees_its_place_and_the_keys() {
    let mut world = wide();
    let (panel, _) = a_panel(&mut world);
    let area = areas(&world).plugin;
    click(&mut world, area);
    let mut ended = panel.clone();
    ended.status = SessionStatus::Exited { success: true };
    ended.ended_at = Some(Timestamp::from_millis(9));
    let actions = upsert(&mut world, ended);
    assert_eq!(world.app.mode, Mode::Browse);
    assert_eq!(sent(&actions, false), [panel.id]);
    assert_eq!(world.app.plugin_stream.session, None);
    assert!(areas(&world).plugin.is_empty());
}

#[test]
fn a_panel_stands_by_its_chat_on_one_line_and_is_never_news() {
    let mut world = wide();
    let (mut panel, _) = a_panel(&mut world);
    let rows = world.app.rows();
    let at = |id| {
        rows.iter()
            .position(|row| *row == Row::Session(id))
            .unwrap()
    };
    assert_eq!(at(panel.id), at(world.session.id) + 1, "{rows:?}");
    assert_eq!(world.app.row_heights(&rows)[at(panel.id)], 1);
    panel.unseen = true;
    upsert(&mut world, panel);
    assert!(world.app.model.needs_you().is_empty());
    let screen = render_in(&world.app, WIDE);
    assert!(screen.contains("no agent at work"), "{screen}");
}

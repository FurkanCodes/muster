//! Shells beside the agents (`PRODUCT.md` §6): `T` places the workspace's
//! shell beside the panel with the keys, both streamed: one running there
//! off the screen, else a new one, and where one is on the screen the keys
//! go to it; the list folds where they do not fit, and the panel with the
//! keys says how many more it shows over; a click moves the keys between
//! them; a shell that ends leaves its panel; a shell is not counted among
//! the agents; and in the list it stands by its chats on one line, under
//! the one chat of its worktree.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEventKind};
use muster_core::layout::{Edge, Panel, Place};
use muster_core::protocol::{ClientFrame, Command, Delta, Reply, ServerFrame, TermSize};
use muster_core::{Session, SessionId, SessionStatus};

use super::pointer_tests::mouse;
use super::tests::{World, command, render_in, world};
use super::{Action, Mode};
use crate::model::Row;
use crate::model::tests::session;

const WIDE: TermSize = TermSize {
    cols: 180,
    rows: 29,
};

fn shift_t() -> KeyEvent {
    KeyEvent::new(KeyCode::Char('T'), KeyModifiers::SHIFT)
}

fn areas(world: &World) -> crate::view::BrowseAreas {
    let size = world.app.terminal_size;
    crate::view::browse_areas(
        &world.app,
        ratatui::layout::Rect::new(0, 0, size.cols, size.rows + 1),
    )
}

/// A shell in the world's worktree, as the state says.
fn a_shell(world: &mut World) -> Session {
    let mut shell = session(world.worktree.id, "zsh", 5);
    shell.harness = "shell".to_owned();
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::SessionUpserted(shell.clone())));
    shell
}

/// The world in a window `size` big, its session selected beside the list.
fn sized(size: TermSize) -> World {
    let mut world = world();
    world.app.selected = Some(Row::Session(world.session.id));
    world.app.on_resize(size);
    world
}

fn attached(actions: &[Action]) -> Vec<SessionId> {
    actions
        .iter()
        .filter_map(|action| match action {
            Action::Send(ClientFrame::Attach { session, .. }) => Some(*session),
            _ => None,
        })
        .collect()
}

/// Whether `session` is placed in a panel of its own.
fn placed(world: &World, session: SessionId) -> bool {
    world.app.panels().placed().contains(&session)
}

#[test]
fn t_starts_a_shell_placed_beside_the_panel_with_the_keys_both_streamed() {
    let mut world = sized(WIDE);
    let (req_id, asked) = command(&world.app.on_key(shift_t()));
    let Command::StartSession {
        workspace, harness, ..
    } = asked
    else {
        panic!("{asked:?}");
    };
    assert_eq!((workspace, harness.as_str()), (world.worktree.id, "shell"));
    let shell_id = SessionId::new();
    world.app.on_frame(ServerFrame::Reply {
        req_id,
        outcome: Ok(Reply::SessionStarted { id: shell_id }),
    });
    let mut shell = session(world.worktree.id, "zsh", 5);
    shell.id = shell_id;
    shell.harness = "shell".to_owned();
    let actions = world
        .app
        .on_frame(ServerFrame::Delta(Delta::SessionUpserted(shell)));
    assert_eq!(world.app.mode, Mode::Terminal(shell_id));
    assert!(placed(&world, shell_id), "{}", world.app.panels());
    assert_eq!(attached(&actions), [shell_id]);
    assert_eq!(world.app.attached, Some(world.session.id), "the agent too");
    let shown = areas(&world);
    let beside = shown.of(Panel::Placed(shell_id));
    assert!(!shown.list.is_empty() && !shown.preview.is_empty() && !beside.is_empty());
    assert!(beside.x > shown.preview.x, "right of the preview");
    let screen = render_in(&world.app, WIDE);
    assert!(screen.contains("zsh"), "{screen}");
    // Among others, the preview says it is one, and each title where it
    // works.
    assert!(screen.contains("preview · "), "{screen}");
    assert!(
        screen.contains(&format!("{}/", world.project.name)),
        "{screen}"
    );
}

#[test]
fn t_places_the_shell_running_off_the_screen_and_again_gives_it_the_keys() {
    let mut world = sized(WIDE);
    let shell = a_shell(&mut world);
    let actions = world.app.on_key(shift_t());
    assert!(
        !actions.iter().any(|action| matches!(
            action,
            Action::Send(ClientFrame::Command {
                command: Command::StartSession { .. },
                ..
            })
        )),
        "none started: {actions:?}"
    );
    assert_eq!(world.app.mode, Mode::Terminal(shell.id));
    assert!(placed(&world, shell.id));
    // Back to the list, and T again: the keys go to it, where it is.
    world.app.on_key(super::tests::shift_q());
    let layout = world.app.panels().clone();
    world.app.on_key(shift_t());
    assert_eq!(world.app.mode, Mode::Terminal(shell.id));
    assert_eq!(*world.app.panels(), layout, "it stays where it is");
}

#[test]
fn where_the_panels_do_not_fit_the_list_folds_then_the_one_with_the_keys_shows_over() {
    let mut world = sized(TermSize {
        cols: 120,
        rows: 29,
    });
    let shell = a_shell(&mut world);
    world.app.on_key(shift_t());
    let shown = areas(&world);
    assert!(shown.list.is_empty(), "the list folded: {shown:?}");
    assert!(!shown.preview.is_empty() && !shown.of(Panel::Placed(shell.id)).is_empty());
    assert_eq!(shown.hidden, 0);
    // Another beside them: the one with the keys shows over the rest, and
    // says how many.
    let mut other = session(world.worktree.id, "bun", 6);
    other.harness = "shell".to_owned();
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::SessionUpserted(other.clone())));
    let actions = world.app.place_at(
        other.id,
        Place::Beside(Panel::Placed(shell.id), Edge::Right),
    );
    drop(actions);
    world.app.mode = Mode::Terminal(other.id);
    let shown = areas(&world);
    assert_eq!(shown.hidden, 2, "{shown:?}");
    assert!(shown.preview.is_empty() && shown.of(Panel::Placed(shell.id)).is_empty());
    let screen = render_in(
        &world.app,
        TermSize {
            cols: 120,
            rows: 29,
        },
    );
    assert!(screen.contains("+2 more"), "{screen}");

    let mut world = sized(TermSize { cols: 90, rows: 29 });
    a_shell(&mut world);
    world.app.on_key(shift_t());
    assert!(world.app.fills_window(), "over the session");
}

#[test]
fn a_click_moves_the_keys_between_the_session_and_its_shell() {
    let mut world = sized(WIDE);
    let shell = a_shell(&mut world);
    world.app.on_key(shift_t());
    let shown = areas(&world);
    let on = |area: ratatui::layout::Rect| (area.x + 5, area.y + 5);
    let (column, row) = on(shown.preview);
    world
        .app
        .on_mouse(mouse(MouseEventKind::Down(MouseButton::Left), column, row));
    assert_eq!(world.app.mode, Mode::Terminal(world.session.id));
    let (column, row) = on(areas(&world).of(Panel::Placed(shell.id)));
    world
        .app
        .on_mouse(mouse(MouseEventKind::Down(MouseButton::Left), column, row));
    assert_eq!(world.app.mode, Mode::Terminal(shell.id));
}

#[test]
fn a_shell_is_not_counted_among_the_agents_nor_waits_on_the_person() {
    let mut world = sized(WIDE);
    let mut shell = a_shell(&mut world);
    shell.status = SessionStatus::Exited { success: false };
    shell.unseen = true;
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::SessionUpserted(shell.clone())));
    let mut running = shell.clone();
    running.id = SessionId::new();
    running.status = SessionStatus::Running;
    running.unseen = false;
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::SessionUpserted(running)));
    assert!(world.app.model.needs_you().is_empty());
    let screen = render_in(&world.app, WIDE);
    assert!(screen.contains("no agent at work"), "{screen}");
}

#[test]
fn a_placed_shell_that_ends_leaves_its_panel_and_the_keys() {
    let mut world = sized(WIDE);
    let shell = a_shell(&mut world);
    world.app.on_key(shift_t());
    assert_eq!(world.app.mode, Mode::Terminal(shell.id));
    let mut ended = shell.clone();
    ended.ended_at = Some(muster_core::Timestamp::from_millis(9));
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::SessionUpserted(ended)));
    assert!(!world.app.placed_beside().contains(&shell.id));
    assert!(areas(&world).of(Panel::Placed(shell.id)).is_empty());
    assert_eq!(world.app.mode, Mode::Browse);
}

/// The rendered list's line that holds `text`.
fn line_with(world: &World, text: &str) -> String {
    let screen = render_in(&world.app, WIDE);
    screen
        .lines()
        .find(|line| line.contains(text))
        .unwrap_or_else(|| panic!("no {text:?} in\n{screen}"))
        .to_owned()
}

#[test]
fn a_worktree_s_shell_stands_under_its_one_chat_on_one_line() {
    let mut world = sized(WIDE);
    // Started before the chat, still listed after it.
    let mut shell = session(world.worktree.id, "cargo", 1);
    shell.harness = "shell".to_owned();
    shell.status = SessionStatus::Running;
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::SessionUpserted(shell.clone())));
    let rows = world.app.rows();
    let at = |id| {
        rows.iter()
            .position(|row| *row == Row::Session(id))
            .unwrap()
    };
    assert_eq!(at(shell.id), at(world.session.id) + 1, "{rows:?}");
    assert_eq!(world.app.row_heights(&rows)[at(shell.id)], 1, "one line");
    let line = line_with(&world, "cargo");
    assert!(line.contains("└ ❯ cargo"), "{line}");

    // A click on its line selects it, the chat above taking two.
    let list = areas(&world).list;
    let offset = world.app.list_offset;
    let lines: usize = world.app.row_heights(&rows)[offset..at(shell.id)]
        .iter()
        .sum();
    let row = list.y + 1 + u16::try_from(lines).unwrap();
    world.app.on_mouse(super::pointer_tests::mouse(
        MouseEventKind::Down(MouseButton::Left),
        list.x + 6,
        row,
    ));
    assert_eq!(world.app.selected, Some(Row::Session(shell.id)));
}

#[test]
fn where_chats_share_a_workspace_its_shell_stands_after_them_all() {
    let mut world = sized(WIDE);
    let mut other = session(world.worktree.id, "Add dark mode", 3);
    other.status = SessionStatus::Finished;
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::SessionUpserted(other.clone())));
    let shell = a_shell(&mut world);
    let rows = world.app.rows();
    assert_eq!(
        rows.iter().rev().find(|row| matches!(row, Row::Session(_))),
        Some(&Row::Session(shell.id)),
        "{rows:?}"
    );
    let line = line_with(&world, "❯ zsh");
    assert!(!line.contains('└'), "beside both chats: {line}");
}

/// Whether `session` is streamed once `actions` are sent, from `was`: the
/// last of its Attach and Detach decides.
fn streamed_after(actions: &[Action], session: SessionId, was: bool) -> bool {
    actions.iter().fold(was, |streamed, action| match action {
        Action::Send(ClientFrame::Attach { session: id, .. }) if *id == session => true,
        Action::Send(ClientFrame::Detach { session: id }) if *id == session => false,
        _ => streamed,
    })
}

#[test]
fn a_placed_shell_keeps_streaming_as_the_selection_moves_between_it_and_its_chat() {
    let mut world = sized(WIDE);
    let shell = a_shell(&mut world);
    world.app.on_key(shift_t());
    world.app.on_key(super::tests::shift_q());
    let chat = world.session.id;
    assert!(placed(&world, shell.id));
    let mut streamed = true;
    let mut chat_streamed = true;
    // Down to the shell's own line: placed, it stays in its panel, and the
    // preview keeps the chat.
    let actions = world
        .app
        .on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    assert_eq!(world.app.selected, Some(Row::Session(shell.id)));
    streamed = streamed_after(&actions, shell.id, streamed);
    chat_streamed = streamed_after(&actions, chat, chat_streamed);
    assert!(streamed && chat_streamed, "both shown: {actions:?}");
    // Up to the chat: both still streamed.
    let actions = world
        .app
        .on_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
    assert_eq!(world.app.selected, Some(Row::Session(chat)));
    streamed = streamed_after(&actions, shell.id, streamed);
    assert!(streamed, "beside the chat: {actions:?}");
    assert!(streamed_after(&actions, chat, chat_streamed), "{actions:?}");
}

#[test]
fn a_placed_shell_streams_again_from_a_new_daemon() {
    let mut world = sized(WIDE);
    let shell = a_shell(&mut world);
    world.app.on_key(shift_t());
    world.app.on_key(super::tests::shift_q());
    assert!(placed(&world, shell.id));
    // The daemon restarted in place: the UI reconnects to a new one, which
    // streams nothing to it yet.
    world.app.on_disconnected("restarting".to_owned());
    world.app.on_connected("0.1.1".to_owned());
    let actions = world
        .app
        .on_frame(ServerFrame::Snapshot(muster_core::protocol::Snapshot {
            projects: vec![world.project.clone()],
            workspaces: vec![world.worktree.clone()],
            sessions: vec![world.session.clone(), shell.clone()],
            ..muster_core::protocol::Snapshot::default()
        }));
    let attached = attached(&actions);
    assert!(attached.contains(&shell.id), "{actions:?}");
    assert!(attached.contains(&world.session.id), "{actions:?}");
}

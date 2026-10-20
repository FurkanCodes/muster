//! A shell beside the agent (`PRODUCT.md` §6): `T` opens the workspace's
//! shell in a panel of its own beside the session, both streamed, or the
//! one it has; the list folds where the three do not fit, and the shell
//! opens over the session where two do not; a click moves the keys
//! between them; a shell is not counted among the agents; and in the list
//! it stands by its chats on one line, under the one chat of its worktree.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEventKind};
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

#[test]
fn t_starts_the_workspace_s_shell_beside_the_session_both_streamed() {
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
    assert_eq!(attached(&actions), [shell_id]);
    assert_eq!(world.app.attached, Some(world.session.id), "the agent too");
    let shown = areas(&world);
    assert!(!shown.list.is_empty() && !shown.preview.is_empty() && !shown.shell.is_empty());
    assert!(shown.shell.x > shown.preview.x, "beside the session");
    let screen = render_in(&world.app, WIDE);
    assert!(screen.contains("zsh"), "{screen}");
}

#[test]
fn t_opens_the_shell_a_workspace_has_and_again_puts_it_away() {
    let mut world = sized(WIDE);
    let shell = a_shell(&mut world);
    let actions = world.app.on_key(shift_t());
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Send(ClientFrame::Command { .. }))),
        "none started: {actions:?}"
    );
    assert_eq!(world.app.mode, Mode::Terminal(shell.id));
    // Back to the list, and T again: put away, still running.
    world.app.on_key(super::tests::shift_q());
    assert_eq!(world.app.shell_beside(), Some(shell.id));
    world.app.on_key(shift_t());
    assert_eq!(world.app.shell_beside(), None);
    assert!(areas(&world).shell.is_empty());
}

#[test]
fn where_three_do_not_fit_the_list_folds_and_where_two_do_not_the_shell_opens_over() {
    let mut world = sized(TermSize {
        cols: 120,
        rows: 29,
    });
    a_shell(&mut world);
    world.app.on_key(shift_t());
    let shown = areas(&world);
    assert!(shown.list.is_empty(), "the list folded: {shown:?}");
    assert!(!shown.preview.is_empty() && !shown.shell.is_empty());

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
    let (column, row) = on(areas(&world).shell);
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
    // An ended shell beside the session goes, with the keys.
    world.app.shell = Some(shell.id);
    world.app.mode = Mode::Terminal(shell.id);
    let mut ended = shell;
    ended.ended_at = Some(muster_core::Timestamp::from_millis(9));
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::SessionUpserted(ended)));
    assert_eq!(world.app.shell, None);
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
fn the_shell_keeps_streaming_as_the_selection_moves_between_it_and_its_chat() {
    let mut world = sized(WIDE);
    let shell = a_shell(&mut world);
    world.app.on_key(shift_t());
    world.app.on_key(super::tests::shift_q());
    let chat = world.session.id;
    assert_eq!(world.app.shell_beside(), Some(shell.id));
    let mut streamed = true;
    // Down to the shell's own line: it is the session shown.
    let actions = world
        .app
        .on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    assert_eq!(world.app.selected, Some(Row::Session(shell.id)));
    streamed = streamed_after(&actions, shell.id, streamed);
    assert!(streamed, "shown: {actions:?}");
    // Up to the chat: the shell beside it again, still streamed.
    let actions = world
        .app
        .on_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
    assert_eq!(world.app.selected, Some(Row::Session(chat)));
    assert_eq!(world.app.shell_beside(), Some(shell.id));
    streamed = streamed_after(&actions, shell.id, streamed);
    assert!(streamed, "beside the chat: {actions:?}");
    assert!(streamed_after(&actions, chat, true), "{actions:?}");
}

#[test]
fn the_shell_beside_the_chat_streams_again_from_a_new_daemon() {
    let mut world = sized(WIDE);
    let shell = a_shell(&mut world);
    world.app.on_key(shift_t());
    world.app.on_key(super::tests::shift_q());
    assert_eq!(world.app.shell_beside(), Some(shell.id));
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

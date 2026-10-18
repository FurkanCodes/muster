//! The workspace's timeline in the changes pane (`PRODUCT.md` §6): its
//! turns listed, a turn's diff, and putting the files back from one only
//! once the person says yes (`PRD-14`).

use crossterm::event::KeyCode;
use muster_core::changes::{ChangeSummary, Changes, FileDiff};
use muster_core::protocol::{
    ClientFrame, Command, Delta, ErrorBody, ErrorCode, Reply, ServerFrame,
};
use muster_core::timeline::{BranchMove, Entry, EntryKind, Timeline, UndoPlan};
use muster_core::{SessionId, Timestamp, Workspace};

use super::super::tests::{World, press, render, world};
use super::super::{Action, Dialog, Mode, Pick};
use crate::model::Row;

const MINUTE: i64 = 60_000;

fn turn(number: u64, session: SessionId, ended: i64) -> Entry {
    Entry {
        number,
        kind: EntryKind::Turn {
            session,
            agent: "Claude Code".to_owned(),
        },
        started: Timestamp::from_millis(ended - MINUTE),
        ended: Timestamp::from_millis(ended),
        summary: Some(ChangeSummary {
            files: 2,
            added: 40,
            removed: 2,
        }),
        head_before: None,
        head_after: None,
    }
}

/// Two turns of one session with the person's edit between, newest first.
fn timeline() -> Timeline {
    let session = SessionId::new();
    Timeline {
        entries: vec![
            turn(3, session, 50 * MINUTE),
            Entry {
                kind: EntryKind::Between,
                summary: None,
                ..turn(2, session, 30 * MINUTE)
            },
            turn(1, session, 20 * MINUTE),
        ],
    }
}

/// Every command in `actions`, with its request id.
fn commands(actions: &[Action]) -> Vec<(u64, Command)> {
    actions
        .iter()
        .filter_map(|action| match action {
            Action::Send(ClientFrame::Command { req_id, command }) => {
                Some((*req_id, command.clone()))
            }
            _ => None,
        })
        .collect()
}

fn reply(world: &mut World, req_id: u64, outcome: Result<Reply, ErrorBody>) -> Vec<Action> {
    world.app.on_frame(ServerFrame::Reply { req_id, outcome })
}

/// `d` on the world's worktree, answered with nothing changed and
/// `timeline()`; the selection put on the newest turn.
fn opened(world: &mut World) {
    world.app.now = Some(Timestamp::from_millis(55 * MINUTE));
    world.app.selected = Some(Row::Worktree(world.worktree.id));
    let asked = commands(&press(&mut world.app, KeyCode::Char('d')));
    let workspace = world.worktree.id;
    assert_eq!(
        asked
            .iter()
            .map(|(_, command)| command.clone())
            .collect::<Vec<_>>(),
        [
            Command::WorkspaceChanges { workspace },
            Command::Timeline { workspace },
        ]
    );
    reply(world, asked[0].0, Ok(Reply::Changes(Changes::default())));
    reply(world, asked[1].0, Ok(Reply::Timeline(timeline())));
}

fn upsert(world: &mut World, change: impl FnOnce(&mut Workspace)) -> Vec<Action> {
    let mut worktree = world
        .app
        .model
        .workspace(world.worktree.id)
        .cloned()
        .unwrap();
    change(&mut worktree);
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::WorkspaceUpserted(worktree)))
}

#[test]
fn the_pane_lists_each_turn_with_what_it_changed_and_when() {
    let mut world = world();
    opened(&mut world);
    let screen = render(&world.app);
    for shown in [
        "Turns · 3",
        "#3  Claude Code · turn 2",
        "#2  Edits between turns",
        "#1  Claude Code · turn 1",
        "5m ago",
        "+40 −2",
        "no files",
        "z put back as before it",
    ] {
        assert!(screen.contains(shown), "{shown:?} in\n{screen}");
    }
    let pane = world.app.changes.as_ref().unwrap();
    assert_eq!(
        pane.selected,
        Some(Pick::Entry(3)),
        "the first selectable line"
    );
}

#[test]
fn enter_on_a_turn_shows_what_it_changed() {
    let mut world = world();
    opened(&mut world);
    let asked = commands(&press(&mut world.app, KeyCode::Enter));
    assert_eq!(
        asked[0].1,
        Command::EntryDiff {
            workspace: world.worktree.id,
            number: 3
        }
    );
    reply(
        &mut world,
        asked[0].0,
        Ok(Reply::FileDiff(FileDiff {
            text: "+fn login() {}\n".to_owned(),
            cut: false,
        })),
    );
    let screen = render(&world.app);
    assert!(screen.contains("#3 · Claude Code · turn 2"), "{screen}");
    assert!(screen.contains("+fn login() {}"), "{screen}");
}

#[test]
fn z_asks_first_and_puts_the_files_back_only_on_yes() {
    let mut world = world();
    opened(&mut world);
    let workspace = world.worktree.id;
    press(&mut world.app, KeyCode::Down);
    press(&mut world.app, KeyCode::Down);
    let asked = commands(&press(&mut world.app, KeyCode::Char('z')));
    assert_eq!(
        asked[0].1,
        Command::UndoPlan {
            workspace,
            number: 1
        }
    );
    reply(
        &mut world,
        asked[0].0,
        Ok(Reply::UndoPlan(UndoPlan {
            number: 1,
            later: 2,
            branch: None,
        })),
    );
    assert!(matches!(world.app.mode, Mode::Dialog(Dialog::Undo { .. })));
    let screen = render(&world.app);
    for shown in [
        "Put the files back?",
        "before #1 (Claude Code · turn 1)",
        "What 2 later entries changed goes too.",
        "Claude Code still remembers that turn",
        "y put the files back",
    ] {
        assert!(screen.contains(shown), "{shown:?} in\n{screen}");
    }
    // The pane stays behind the question.
    assert!(screen.contains("Turns · 3"), "{screen}");

    // No: nothing is sent, and the pane has the keys again.
    assert!(commands(&press(&mut world.app, KeyCode::Char('n'))).is_empty());
    assert_eq!(world.app.mode, Mode::Changes);
    // A branch that cannot move is not offered.
    let asked = commands(&press(&mut world.app, KeyCode::Char('z')));
    reply(
        &mut world,
        asked[0].0,
        Ok(Reply::UndoPlan(UndoPlan {
            number: 1,
            later: 2,
            branch: Some(BranchMove {
                branch: "work".to_owned(),
                to: "a1b2c3d".to_owned(),
                commits: 2,
                forward: false,
                refused: Some("its commits since are pushed".to_owned()),
            }),
        })),
    );
    assert!(render(&world.app).contains("work stays where it is: its commits since are pushed."));
    assert!(commands(&press(&mut world.app, KeyCode::Char('b'))).is_empty());
    let asked = commands(&press(&mut world.app, KeyCode::Char('y')));
    assert_eq!(
        asked[0].1,
        Command::Undo {
            workspace,
            number: 1,
            move_branch: false
        }
    );
    assert_eq!(world.app.mode, Mode::Changes);

    // Done: said, and what is there now asked for.
    let after = commands(&reply(
        &mut world,
        asked[0].0,
        Ok(Reply::Undone {
            entry: 4,
            moved_branch: false,
        }),
    ));
    let screen = render(&world.app);
    assert!(
        screen.contains("The files are as they were before #1. z on #4 brings them back."),
        "{screen}"
    );
    let asked: Vec<Command> = after.into_iter().map(|(_, command)| command).collect();
    assert!(
        asked.contains(&Command::WorkspaceChanges { workspace }),
        "{asked:?}"
    );
    assert!(
        asked.contains(&Command::Timeline { workspace }),
        "{asked:?}"
    );
}

#[test]
fn b_moves_the_branch_too_where_it_can_go() {
    let mut world = world();
    opened(&mut world);
    let workspace = world.worktree.id;
    let asked = commands(&press(&mut world.app, KeyCode::Char('z')));
    reply(
        &mut world,
        asked[0].0,
        Ok(Reply::UndoPlan(UndoPlan {
            number: 3,
            later: 0,
            branch: Some(BranchMove {
                branch: "work".to_owned(),
                to: "a1b2c3d".to_owned(),
                commits: 1,
                forward: false,
                refused: None,
            }),
        })),
    );
    let screen = render(&world.app);
    assert!(
        screen.contains("b also moves work back to a1b2c3d, before its 1 commit since."),
        "{screen}"
    );
    let asked = commands(&press(&mut world.app, KeyCode::Char('b')));
    assert_eq!(
        asked[0].1,
        Command::Undo {
            workspace,
            number: 3,
            move_branch: true
        }
    );
    // Refused: said, and nothing else.
    reply(
        &mut world,
        asked[0].0,
        Err(ErrorBody::new(
            ErrorCode::Conflict,
            "Claude Code is at work there: put the files back once it has stopped",
        )),
    );
    assert!(render(&world.app).contains("put the files back once it has stopped"));
}

#[test]
fn z_on_a_file_says_to_choose_a_turn() {
    let mut world = world();
    world.app.selected = Some(Row::Worktree(world.worktree.id));
    let asked = commands(&press(&mut world.app, KeyCode::Char('d')));
    reply(
        &mut world,
        asked[1].0,
        Ok(Reply::Timeline(Timeline::default())),
    );
    let screen = render(&world.app);
    assert!(screen.contains("None kept yet"), "{screen}");
    assert!(commands(&press(&mut world.app, KeyCode::Char('z'))).is_empty());
    assert!(render(&world.app).contains("Choose a turn under Turns"));
}

#[test]
fn a_new_entry_in_the_state_asks_for_the_timeline_again() {
    let mut world = world();
    opened(&mut world);
    let asked = commands(&upsert(&mut world, |worktree| worktree.timeline = Some(4)));
    assert_eq!(
        asked
            .iter()
            .map(|(_, command)| command.clone())
            .collect::<Vec<_>>(),
        [Command::Timeline {
            workspace: world.worktree.id
        }]
    );
    // Told again of the same, it does not ask again.
    assert!(commands(&upsert(&mut world, |worktree| worktree.timeline = Some(4))).is_empty());
}

#[test]
fn an_older_daemon_is_not_asked_for_a_timeline() {
    let mut world = world();
    world.app.daemon_protocol = muster_core::changes::CHANGES_PROTOCOL;
    world.app.selected = Some(Row::Worktree(world.worktree.id));
    let asked = commands(&press(&mut world.app, KeyCode::Char('d')));
    assert_eq!(asked.len(), 1, "{asked:?}");
    assert!(commands(&press(&mut world.app, KeyCode::Char('z'))).is_empty());
    assert!(render(&world.app).contains("`muster reload`"));
}

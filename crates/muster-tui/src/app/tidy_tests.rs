//! Tidying a branch's commits (`PRODUCT.md` §6, `PRD-14`): the agent
//! asked for first, the daemon asked only on ⏎, and what the tidy came to
//! on its session.

use crossterm::event::KeyCode;
use muster_core::protocol::{Command, Delta, ServerFrame};
use muster_core::tidy::{Tidy, TidyCheck};
use muster_core::{Session, SessionStatus};

use super::super::tests::{World, command, press, render, world};
use super::super::{Dialog, HarnessChoice, Mode};
use crate::model::Row;

#[test]
fn t_asks_which_agent_tidies_and_only_then_starts_it() {
    let mut world = world();
    world.app.selected = Some(Row::Worktree(world.worktree.id));
    assert!(press(&mut world.app, KeyCode::Char('t')).is_empty());
    assert_eq!(
        world.app.mode,
        Mode::Dialog(Dialog::Tidy {
            workspace: world.worktree.id,
            harness: HarnessChoice::Claude,
        })
    );
    let screen = render(&world.app);
    for shown in [
        "Tidy the commits",
        "the commits on work not pushed yet",
        "Tidy with  Claude Code",
        "⏎ start",
    ] {
        assert!(screen.contains(shown), "{shown:?} in\n{screen}");
    }
    // A shell tidies nothing: the choice goes round the agents.
    press(&mut world.app, KeyCode::BackTab);
    assert!(matches!(
        world.app.mode,
        Mode::Dialog(Dialog::Tidy {
            harness: HarnessChoice::Copilot,
            ..
        })
    ));
    press(&mut world.app, KeyCode::Tab);
    press(&mut world.app, KeyCode::Tab);
    let (_, asked) = command(&press(&mut world.app, KeyCode::Enter));
    assert_eq!(
        asked,
        Command::TidyCommits {
            workspace: world.worktree.id,
            harness: "codex".to_owned(),
            size: world.app.session_size(),
        }
    );
    assert_eq!(world.app.mode, Mode::Browse);
}

#[test]
fn esc_asks_nothing() {
    let mut world = world();
    world.app.selected = Some(Row::Worktree(world.worktree.id));
    press(&mut world.app, KeyCode::Char('t'));
    assert!(press(&mut world.app, KeyCode::Esc).is_empty());
    assert_eq!(world.app.mode, Mode::Browse);
}

fn tidied(world: &mut World, status: SessionStatus, checked: Option<TidyCheck>) -> String {
    let session = Session {
        status,
        tidy: Some(Tidy {
            commits: 5,
            from: "a1b2c3d".to_owned(),
            checked,
        }),
        ..world.session.clone()
    };
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::SessionUpserted(session)));
    world.app.selected = Some(Row::Session(world.session.id));
    render(&world.app)
}

#[test]
fn a_tidy_says_on_its_session_what_it_came_to() {
    let mut world = world();
    let screen = tidied(&mut world, SessionStatus::Running, None);
    assert!(
        screen.contains("tidying 5 commits after a1b2c3d"),
        "{screen}"
    );
    let screen = tidied(
        &mut world,
        SessionStatus::Finished,
        Some(TidyCheck::Kept { commits: 2 }),
    );
    assert!(
        screen.contains("tidied 5 commits into 2, files unchanged"),
        "{screen}"
    );
    let screen = tidied(
        &mut world,
        SessionStatus::Finished,
        Some(TidyCheck::FilesChanged { files: 3 }),
    );
    assert!(
        screen.contains("the tidy changed 3 files: z in d puts them back"),
        "{screen}"
    );
}

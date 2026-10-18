//! Where a worktree's work meets other work in its project (`PRODUCT.md`
//! §6): a word on its line, a line in its details, and the other work
//! first in its changes pane, all as the state says.

use crossterm::event::KeyCode;
use muster_core::changes::{ChangeSummary, Changes};
use muster_core::clashes::{Clash, ClashFile};
use muster_core::protocol::{Delta, Reply, ServerFrame};
use muster_core::{Workspace, WorkspaceId};

use super::tests::{World, command, press, render, world};
use super::{Action, Pick};
use crate::model::Row;
use crate::model::tests::worktree;

fn clash(with: WorkspaceId, files: &[(&str, bool)]) -> Clash {
    Clash {
        with,
        files: u64::try_from(files.len()).unwrap(),
        conflicts: u64::try_from(files.iter().filter(|(_, conflict)| *conflict).count()).unwrap(),
        shown: files
            .iter()
            .map(|(path, conflict)| ClashFile {
                path: (*path).to_owned(),
                conflict: *conflict,
            })
            .collect(),
    }
}

/// The world with a second worktree, on `other`, whose work meets the
/// world's worktree's in `files`.
fn meeting(files: &[(&str, bool)]) -> (World, Workspace) {
    let mut world = world();
    let mut other = worktree(world.project.id, "other");
    other.clashes = vec![clash(world.worktree.id, files)];
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::WorkspaceUpserted(other.clone())));
    let with = other.id;
    upsert(&mut world, |worktree| {
        worktree.changes = Some(ChangeSummary {
            files: 2,
            added: 12,
            removed: 3,
        });
        worktree.clashes = vec![clash(with, files)];
    });
    (world, other)
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
fn work_that_would_conflict_says_so_on_its_line_and_in_its_details() {
    let (mut world, _) = meeting(&[("src/view.rs", true), ("README.md", false)]);
    world.app.selected = Some(Row::Worktree(world.worktree.id));
    let screen = render(&world.app);
    let line = screen.lines().find(|line| line.contains("⎇ work")).unwrap();
    assert!(line.contains("conflict +12 −3"), "{line}");
    assert!(
        screen.contains("Clashes ⎇ other would conflict in src/view.rs"),
        "{screen}"
    );
}

#[test]
fn work_on_the_same_files_alone_is_a_milder_word() {
    let (mut world, _) = meeting(&[("notes.md", false)]);
    world.app.selected = Some(Row::Worktree(world.worktree.id));
    let screen = render(&world.app);
    let line = screen.lines().find(|line| line.contains("⎇ work")).unwrap();
    assert!(line.contains("overlap"), "{line}");
    assert!(!line.contains("conflict"), "{line}");
    assert!(
        screen.contains("Clashes ⎇ other changes them too: notes.md"),
        "{screen}"
    );
}

#[test]
fn the_changes_pane_shows_other_work_first_and_follows_the_state() {
    let (mut world, _) = meeting(&[("src/view.rs", true)]);
    world.app.selected = Some(Row::Worktree(world.worktree.id));
    let (req_id, _) = command(&press(&mut world.app, KeyCode::Char('d')));
    world.app.on_frame(ServerFrame::Reply {
        req_id,
        outcome: Ok(Reply::Changes(Changes {
            base: Some("main".to_owned()),
            ..Changes::default()
        })),
    });
    let screen = render(&world.app);
    for shown in [
        "Other work on these files · 1",
        "⎇ other  would conflict in src/view.rs",
    ] {
        assert!(screen.contains(shown), "{shown:?} in\n{screen}");
    }
    let pane = world.app.changes.as_ref().unwrap();
    assert!(
        !matches!(pane.selected, Some(Pick::File(..))),
        "another's work is not a file of this one's to choose"
    );

    // The other's work moves on: the pane follows the state, asking
    // nothing of the daemon.
    let asked = upsert(&mut world, |worktree| worktree.clashes = Vec::new());
    assert!(
        !asked.iter().any(|action| matches!(action, Action::Send(_))),
        "{asked:?}"
    );
    assert!(!render(&world.app).contains("Other work on these files"));
}

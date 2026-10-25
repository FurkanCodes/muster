//! The pull request from each worktree's branch, as the daemon last
//! looked: beside the worktree, in its details, and `b` to open it.

use crossterm::event::KeyCode;
use muster_core::protocol::{Delta, ServerFrame};
use muster_core::{Checks, PullRequestState, PullRequestStatus, Review, Workspace, WorkspaceKind};

use super::Action;
use super::tests::{World, press, render, world};
use crate::model::Row;

pub(super) fn approved() -> PullRequestStatus {
    PullRequestStatus {
        number: 42,
        url: "https://github.com/o/app/pull/42".to_owned(),
        state: PullRequestState::Open,
        checks: Some(Checks::Passing),
        review: Some(Review::Approved),
    }
}

/// The world's worktree, on `branch`, with `found` as its pull request.
fn shown_with(world: &mut World, branch: &str, found: Option<PullRequestStatus>) {
    let mut worktree: Workspace = world.worktree.clone();
    if let WorkspaceKind::Worktree { branch: on, .. } = &mut worktree.kind {
        branch.clone_into(on);
    }
    worktree.pull_request = found;
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::WorkspaceUpserted(worktree)));
}

/// The screen's line holding `text`.
fn line_with(screen: &str, text: &str) -> String {
    screen
        .lines()
        .find(|line| line.contains(text))
        .unwrap_or_else(|| panic!("no line with {text:?} in\n{screen}"))
        .to_owned()
}

#[test]
fn a_worktree_shows_its_branch_s_pull_request_and_b_opens_it() {
    let mut world = world();
    shown_with(&mut world, "work", Some(approved()));
    world.app.selected = Some(Row::Worktree(world.worktree.id));
    let screen = render(&world.app);
    let row = line_with(&screen, "⎇ work");
    assert!(row.contains("#42 approved ✓"), "{row}");
    assert!(
        screen.contains("PR      #42 · open · checks passed · approved"),
        "{screen}"
    );
    assert!(screen.contains("b opens its pull request in the browser."));
    // Its key among the hints, where they have the room.
    let wide = muster_core::protocol::TermSize {
        cols: 130,
        rows: 29,
    };
    world.app.on_resize(wide);
    let screen = super::tests::render_in(&world.app, wide);
    assert!(screen.contains("b browser"), "{screen}");
    assert_eq!(
        press(&mut world.app, KeyCode::Char('b')),
        [Action::Open("https://github.com/o/app/pull/42".to_owned())]
    );
    // From one of its sessions too (whose terminal comes into view).
    world.app.selected = Some(Row::Session(world.session.id));
    let actions = press(&mut world.app, KeyCode::Char('b'));
    assert_eq!(
        actions.first(),
        Some(&Action::Open("https://github.com/o/app/pull/42".to_owned()))
    );
}

#[test]
fn without_a_pull_request_nothing_is_shown_and_b_says_so() {
    let mut world = world();
    world.app.selected = Some(Row::Worktree(world.worktree.id));
    let screen = render(&world.app);
    assert!(!screen.contains("│PR      "), "{screen}");
    assert!(!screen.contains("b browser"), "{screen}");
    assert!(press(&mut world.app, KeyCode::Char('b')).is_empty());
    let note = world.app.note.clone().unwrap();
    assert!(note.error);
    assert_eq!(
        note.text,
        "No pull request from this branch is known; o opens one."
    );
}

#[test]
fn short_of_room_the_word_goes_first_then_the_branch_is_cut() {
    let mut world = world();
    let branch = "muster/issue-42-fix-the-login-redirect";
    shown_with(&mut world, branch, Some(approved()));
    let row = line_with(&render(&world.app), "⎇ muster/");
    assert!(row.contains("#42 ✓"), "{row}");
    assert!(!row.contains("approved"), "{row}");
    assert!(row.contains('…'), "{row}");
    let mut merged = approved();
    merged.state = PullRequestState::Merged;
    shown_with(&mut world, branch, Some(merged));
    let row = line_with(&render(&world.app), "⎇ muster/");
    assert!(row.contains("#42 merged"), "{row}");
}

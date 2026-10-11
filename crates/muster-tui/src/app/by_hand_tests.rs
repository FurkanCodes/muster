//! The stages by hand on a workspace, from the UI: the checks, a review
//! and a draft pull request.

use crossterm::event::KeyCode;
use muster_core::PullRequest;
use muster_core::protocol::{Command, Delta, Reply, ServerFrame};

use super::Mode;
use super::tests::{command, press, render, world};
use crate::model::tests::session;

fn reply(
    world: &mut super::tests::World,
    req_id: u64,
    outcome: Result<Reply, muster_core::protocol::ErrorBody>,
) {
    world.app.on_frame(ServerFrame::Reply { req_id, outcome });
}

#[test]
fn c_runs_the_checks_in_the_selected_workspace_and_opens_them() {
    let mut world = world();
    world.app.selected = Some(crate::model::Row::Worktree(world.worktree.id));
    assert!(render(&world.app).contains("c check"));
    let actions = press(&mut world.app, KeyCode::Char('c'));
    let (req_id, sent) = command(&actions);
    assert!(matches!(
        sent,
        Command::CheckWorkspace { workspace, .. } if workspace == world.worktree.id
    ));
    let mut checks = session(world.worktree.id, "Checks", 9);
    checks.harness = "checks".to_owned();
    reply(
        &mut world,
        req_id,
        Ok(Reply::SessionStarted { id: checks.id }),
    );
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::SessionUpserted(checks.clone())));
    assert_eq!(world.app.mode, Mode::Terminal(checks.id));
}

#[test]
fn v_reviews_and_o_opens_a_draft_pull_request_from_the_selected_workspace() {
    let mut world = world();
    world.app.selected = Some(crate::model::Row::Session(world.session.id));
    let actions = press(&mut world.app, KeyCode::Char('v'));
    assert!(matches!(
        command(&actions).1,
        Command::ReviewWorkspace { workspace, .. } if workspace == world.worktree.id
    ));
    let actions = press(&mut world.app, KeyCode::Char('o'));
    let (req_id, sent) = command(&actions);
    assert_eq!(
        sent,
        Command::OpenPullRequest {
            workspace: world.worktree.id
        }
    );
    reply(
        &mut world,
        req_id,
        Ok(Reply::PullRequestOpened {
            pr: PullRequest {
                number: 17,
                url: "https://github.com/o/app/pull/17".to_owned(),
            },
        }),
    );
    let note = world.app.note.clone().unwrap();
    assert!(!note.error);
    assert_eq!(
        note.text,
        "Draft pull request #17 is open: https://github.com/o/app/pull/17"
    );
}

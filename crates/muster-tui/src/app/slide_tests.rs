//! A name cut short on the selected line slides along, so it can be read
//! whole: a session's title, or a worktree's branch. Only that line's
//! moves, only while the list has the keys and the window is looked at,
//! and nothing slides with motion off.

use muster_core::protocol::{Delta, ServerFrame, Snapshot};
use muster_core::{SessionStatus, WaitReason, Workspace};

use super::tests::{World, render};
use super::{Mode, View};
use crate::model::Row;
use crate::model::tests::{project, session, worktree};

const TITLE: &str = "Fix the login redirect loop on Safari";
const BRANCH: &str = "muster/issue-2640-fix-the-login-redirect-loop";

/// An app with one project, a worktree on a long branch and a session
/// with a long title in it, and another with a short one.
fn long_world() -> (World, muster_core::Session) {
    let mut app = super::App::new(std::path::PathBuf::from("/src"), super::tests::SIZE);
    app.on_connected("0.1.0".to_owned());
    let project = project("app");
    let worktree = worktree(project.id, BRANCH);
    let long = session(worktree.id, TITLE, 3);
    let short = session(worktree.id, "Shell", 4);
    app.on_frame(ServerFrame::Snapshot(Snapshot {
        projects: vec![project.clone()],
        workspaces: vec![worktree.clone()],
        sessions: vec![long.clone(), short.clone()],
        ..Snapshot::default()
    }));
    // Beside what each is doing, on one line, where names are cut most.
    super::tests::one_line(&mut app);
    let world = World {
        app,
        project,
        worktree,
        session: long,
    };
    (world, short)
}

#[test]
fn a_long_title_on_the_selected_line_rests_slides_to_its_end_and_starts_again() {
    let (mut world, _) = long_world();
    world.app.selected = Some(Row::Session(world.session.id));
    // Twenty cells for the title beside its status and its agent's mark
    // in the 36-column list.
    assert_eq!(crate::view::cut_name(&world.app), Some((37, 20)));
    assert!(world.app.wants_steps(), "a cut name asks for steps");

    assert!(!world.app.on_step(100), "first seen selected: it rests");
    assert!(render(&world.app).contains("Fix the login redir…  idle"));
    assert!(!world.app.on_step(116), "still resting");
    assert!(world.app.on_step(117), "it moves, and asks for a frame");
    assert!(render(&world.app).contains("…x the login redire…  idle"));
    assert!(!world.app.on_step(118), "a character each two steps");
    assert!(world.app.on_step(119));
    assert!(world.app.on_step(121));
    assert!(render(&world.app).contains("…the login redirect…  idle"));

    // Its end in view, it rests there, then starts again.
    world.app.on_step(115 + 34);
    let screen = render(&world.app);
    assert!(screen.contains("…rect loop on Safari  idle"), "{screen}");
    assert!(!world.app.on_step(115 + 34 + 14), "resting at its end");
    assert!(world.app.on_step(100 + 15 + 34 + 15));
    assert!(render(&world.app).contains("Fix the login redir…  idle"));
}

#[test]
fn a_long_branch_on_the_selected_worktree_line_slides() {
    let (mut world, _) = long_world();
    let screen = render(&world.app);
    // Cut short with an ellipsis, as a title is, rather than at the border.
    assert!(
        screen.contains("⎇ muster/issue-2640-fix-the-l…"),
        "{screen}"
    );

    world.app.selected = Some(Row::Worktree(world.worktree.id));
    world.app.on_step(0);
    world.app.on_step(15 + 2 * 17);
    let screen = render(&world.app);
    assert!(
        screen.contains("⎇ …fix-the-login-redirect-loop"),
        "{screen}"
    );
}

#[test]
fn a_branch_slides_beside_its_pull_request() {
    let (mut world, _) = long_world();
    let mut opened: Workspace = world.worktree.clone();
    opened.pull_request = Some(muster_core::PullRequestStatus {
        number: 42,
        url: "https://example.com/pull/42".to_owned(),
        state: muster_core::PullRequestState::Merged,
        checks: None,
        review: None,
    });
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::WorkspaceUpserted(opened)));
    world.app.selected = Some(Row::Worktree(world.worktree.id));
    // Seventeen cells for the branch, beside the pull request.
    assert_eq!(crate::view::cut_name(&world.app), Some((45, 17)));
    world.app.on_step(0);
    assert!(render(&world.app).contains("⎇ muster/issue-264… #42 merged"));
    world.app.on_step(15 + 2 * 28);
    let screen = render(&world.app);
    assert!(
        screen.contains("⎇ …in-redirect-loop #42 merged"),
        "{screen}"
    );
}

#[test]
fn only_the_selected_line_slides() {
    let (mut world, short) = long_world();
    world.app.selected = Some(Row::Session(short.id));
    assert_eq!(crate::view::cut_name(&world.app), None);
    assert!(
        !world.app.wants_steps(),
        "a name that fits asks for nothing"
    );
    world.app.on_step(0);
    world.app.on_step(40);
    let screen = render(&world.app);
    assert!(screen.contains("Fix the login redir…  idle"), "{screen}");
    assert!(
        screen.contains("⎇ muster/issue-2640-fix-the-l…"),
        "{screen}"
    );
}

#[test]
fn a_line_left_and_come_back_to_starts_again() {
    let (mut world, short) = long_world();
    let long = Row::Session(world.session.id);
    world.app.selected = Some(long);
    world.app.on_step(0);
    world.app.on_step(25);
    assert_eq!(world.app.slid_by(View::Sessions, long), 5);
    // Moved away: the runtime's next step sees it.
    world.app.selected = Some(Row::Session(short.id));
    assert_eq!(world.app.slid_by(View::Sessions, long), 0);
    world.app.on_step(26);
    world.app.selected = Some(long);
    world.app.on_step(27);
    assert_eq!(world.app.slid_by(View::Sessions, long), 0);
    world.app.on_step(27 + 17);
    assert_eq!(world.app.slid_by(View::Sessions, long), 1);
}

#[test]
fn nothing_slides_while_a_session_has_the_keys_or_the_window_is_away() {
    let (mut world, _) = long_world();
    let long = Row::Session(world.session.id);
    world.app.selected = Some(long);
    world.app.on_step(0);
    world.app.on_step(25);
    assert_eq!(world.app.slid_by(View::Sessions, long), 5);

    world.app.mode = Mode::Terminal(world.session.id);
    assert_eq!(
        world.app.slid_by(View::Sessions, long),
        0,
        "back to its start"
    );
    assert!(!world.app.wants_steps());
    assert!(!world.app.on_step(26));

    world.app.mode = Mode::Browse;
    world.app.focused = Some(false);
    world.app.on_step(27);
    assert_eq!(world.app.slid_by(View::Sessions, long), 0);
    assert!(!world.app.wants_steps(), "nobody looks: no steps");

    world.app.focused = Some(true);
    assert!(world.app.wants_steps());
    world.app.on_step(30);
    world.app.on_step(30 + 17);
    assert_eq!(world.app.slid_by(View::Sessions, long), 1, "from its start");
}

#[test]
fn with_motion_off_a_cut_name_stays_cut() {
    let (mut world, _) = long_world();
    world.app.motion = false;
    world.app.selected = Some(Row::Session(world.session.id));
    assert!(!world.app.wants_steps());
    world.app.on_step(0);
    assert!(!world.app.on_step(40));
    assert!(render(&world.app).contains("Fix the login redir…  idle"));
}

#[test]
fn a_long_title_slides_on_the_selected_line_of_needs_you() {
    let (mut world, _) = long_world();
    let mut waiting = world.session.clone();
    waiting.status = SessionStatus::NeedsYou {
        reason: WaitReason::Question,
    };
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::SessionUpserted(waiting)));
    world
        .app
        .on_key(super::tests::key(crossterm::event::KeyCode::Char('2')));
    assert_eq!(world.app.view, View::NeedsYou);
    assert_eq!(world.app.needs_selected, Some(world.session.id));
    let (len, room) = crate::view::cut_name(&world.app).expect("cut short");
    let start: String = TITLE.chars().take(room - 1).collect();
    assert!(render(&world.app).contains(&format!("{start}…")));

    world.app.on_step(0);
    world
        .app
        .on_step(15 + 2 * u64::try_from(len - room).unwrap());
    let end: String = TITLE.chars().skip(len - room + 1).collect();
    let screen = render(&world.app);
    assert!(screen.contains(&format!("…{end}")), "{screen}");
    assert!(!screen.contains(&format!("{start}…")), "{screen}");
}

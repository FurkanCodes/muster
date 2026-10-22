//! A workspace's changes beside the list (`d`), its files' diffs, and what
//! its work comes to on its line in the list (`PRODUCT.md` §6).

use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use muster_core::changes::{
    ChangeKind, ChangePart, ChangeSummary, Changes, Commit, FileChange, FileDiff, LineCounts,
};
use muster_core::protocol::{Command, Delta, ErrorBody, ErrorCode, Reply, ServerFrame};
use muster_core::{PullRequestState, PullRequestStatus, Workspace, WorkspaceKind};

use super::tests::{World, command, press, render, shift_q, world};
use super::{Action, Mode, Pick};
use crate::model::Row;

fn file(path: &str, kind: ChangeKind, added: u64, removed: u64) -> FileChange {
    FileChange {
        path: path.to_owned(),
        from: None,
        kind,
        lines: Some(LineCounts { added, removed }),
    }
}

fn changes() -> Changes {
    Changes {
        base: Some("origin/main".to_owned()),
        uncommitted: vec![
            file("src/view.rs", ChangeKind::Modified, 12, 3),
            file("notes.md", ChangeKind::Untracked, 4, 0),
        ],
        commits: vec![Commit {
            id: "a1b2c3d".to_owned(),
            subject: "Fix the login redirect".to_owned(),
        }],
        more_commits: 0,
        branch: vec![file("src/login.rs", ChangeKind::Added, 40, 0)],
    }
}

/// The world's worktree, as the daemon says it is now; what the app sends.
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

fn reply(world: &mut World, req_id: u64, outcome: Result<Reply, ErrorBody>) -> Vec<Action> {
    world.app.on_frame(ServerFrame::Reply { req_id, outcome })
}

/// `d` on the world's worktree, answered with `changes()`.
fn opened(world: &mut World) {
    world.app.selected = Some(Row::Worktree(world.worktree.id));
    let (req_id, asked) = command(&press(&mut world.app, KeyCode::Char('d')));
    assert_eq!(
        asked,
        Command::WorkspaceChanges {
            workspace: world.worktree.id
        }
    );
    assert_eq!(world.app.mode, Mode::Changes);
    reply(world, req_id, Ok(Reply::Changes(changes())));
}

#[test]
fn d_lists_what_is_not_committed_and_the_branch_s_commits_and_files() {
    let mut world = world();
    opened(&mut world);
    let screen = render(&world.app);
    for shown in [
        "Changes · work",
        "Not committed · 2 files",
        "?  notes.md",
        "M  src/view.rs",
        "+12 −3",
        "Commits since origin/main · 1",
        "a1b2c3d Fix the login redirect",
        "Changed on the branch · 1 file",
        "A  src/login.rs",
        "+40",
        "⏎ its diff",
    ] {
        assert!(screen.contains(shown), "{shown:?} in\n{screen}");
    }
    let pane = world.app.changes.as_ref().unwrap();
    assert_eq!(
        pane.selected,
        Some(Pick::File(
            ChangePart::Uncommitted,
            "src/view.rs".to_owned()
        )),
        "the first file listed"
    );
}

#[test]
fn enter_shows_a_file_s_diff_and_esc_goes_back_step_by_step() {
    let mut world = world();
    opened(&mut world);
    press(&mut world.app, KeyCode::Down);
    press(&mut world.app, KeyCode::Down);
    let (req_id, asked) = command(&press(&mut world.app, KeyCode::Enter));
    assert_eq!(
        asked,
        Command::FileDiff {
            workspace: world.worktree.id,
            part: ChangePart::Branch,
            path: "src/login.rs".to_owned(),
        }
    );
    assert!(render(&world.app).contains("Loading diff"));
    let text = "diff --git a/src/login.rs b/src/login.rs\n@@ -0,0 +1,2 @@\n+fn login() {}\n+fn logout() {}\n";
    reply(
        &mut world,
        req_id,
        Ok(Reply::FileDiff(FileDiff {
            text: text.to_owned(),
            cut: false,
        })),
    );
    let screen = render(&world.app);
    assert!(screen.contains("src/login.rs · on the branch"), "{screen}");
    assert!(screen.contains("+fn logout() {}"), "{screen}");
    assert!(screen.contains("esc the changes"), "{screen}");

    press(&mut world.app, KeyCode::Esc);
    assert_eq!(world.app.mode, Mode::Changes);
    assert!(render(&world.app).contains("Changed on the branch"));
    press(&mut world.app, KeyCode::Esc);
    assert_eq!(world.app.mode, Mode::Browse);
    assert!(world.app.changes.is_none());
}

#[test]
fn a_long_diff_scrolls_and_a_cut_one_says_so_at_its_end() {
    let mut world = world();
    opened(&mut world);
    let (req_id, _) = command(&press(&mut world.app, KeyCode::Enter));
    let text = (0..100)
        .map(|n| format!("+line {n}"))
        .collect::<Vec<_>>()
        .join("\n");
    reply(
        &mut world,
        req_id,
        Ok(Reply::FileDiff(FileDiff { text, cut: true })),
    );
    assert!(render(&world.app).contains("+line 0 "));
    press(&mut world.app, KeyCode::End);
    let screen = render(&world.app);
    assert!(screen.contains("+line 99"), "{screen}");
    assert!(screen.contains("cut here"), "{screen}");
    assert!(!screen.contains("+line 0 "), "{screen}");
    press(&mut world.app, KeyCode::Home);
    assert!(render(&world.app).contains("+line 0 "));
}

#[test]
fn d_or_shift_q_gives_the_list_the_keys_back() {
    let mut world = world();
    opened(&mut world);
    press(&mut world.app, KeyCode::Char('d'));
    assert_eq!(world.app.mode, Mode::Browse);
    opened(&mut world);
    world.app.on_key(shift_q());
    assert_eq!(world.app.mode, Mode::Browse);
    assert!(world.app.changes.is_none());
}

#[test]
fn a_changed_summary_asks_again_but_the_listing_s_own_does_not() {
    let mut world = world();
    opened(&mut world);
    // The daemon tells the state what it just answered: nothing new.
    let answered = changes().summary();
    let actions = upsert(&mut world, |worktree| worktree.changes = answered);
    assert!(actions.is_empty(), "{actions:?}");
    // An agent's turn ended there and changed more.
    let actions = upsert(&mut world, |worktree| {
        worktree.changes = Some(ChangeSummary {
            files: 4,
            added: 60,
            removed: 3,
        });
    });
    let (_, asked) = command(&actions);
    assert_eq!(
        asked,
        Command::WorkspaceChanges {
            workspace: world.worktree.id
        }
    );
}

#[test]
fn the_pane_closes_when_its_worktree_goes() {
    let mut world = world();
    opened(&mut world);
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::WorkspaceRemoved {
            id: world.worktree.id,
        }));
    assert_eq!(world.app.mode, Mode::Browse);
    assert!(world.app.changes.is_none());
}

#[test]
fn a_failure_is_shown_in_the_pane_and_r_asks_again() {
    let mut world = world();
    world.app.selected = Some(Row::Worktree(world.worktree.id));
    let (req_id, _) = command(&press(&mut world.app, KeyCode::Char('d')));
    reply(
        &mut world,
        req_id,
        Err(ErrorBody::new(ErrorCode::Internal, "`git diff` failed")),
    );
    assert!(render(&world.app).contains("`git diff` failed"));
    let (_, asked) = command(&press(&mut world.app, KeyCode::Char('r')));
    assert!(matches!(asked, Command::WorkspaceChanges { .. }));
}

#[test]
fn d_is_refused_where_git_cannot_tell_and_by_an_older_daemon() {
    let mut world = world();
    world.app.selected = Some(Row::Worktree(world.worktree.id));
    world.app.daemon_protocol = 23;
    assert!(press(&mut world.app, KeyCode::Char('d')).is_empty());
    assert!(
        world
            .app
            .note
            .as_ref()
            .unwrap()
            .text
            .contains("muster reload")
    );
    assert_eq!(world.app.mode, Mode::Browse);

    world.app.daemon_protocol = muster_core::protocol::PROTOCOL_VERSION;
    upsert(&mut world, |worktree| {
        worktree.kind = WorkspaceKind::Checkout {
            path: "/src/notes".into(),
            branch: String::new(),
        };
    });
    assert!(press(&mut world.app, KeyCode::Char('d')).is_empty());
    assert!(
        world
            .app
            .note
            .as_ref()
            .unwrap()
            .text
            .contains("not a git repository")
    );
}

#[test]
fn d_on_a_session_shows_the_changes_of_its_workspace() {
    let mut world = world();
    world.app.selected = Some(Row::Session(world.session.id));
    let (_, asked) = command(&press(&mut world.app, KeyCode::Char('d')));
    assert_eq!(
        asked,
        Command::WorkspaceChanges {
            workspace: world.worktree.id
        }
    );
}

#[test]
fn a_click_on_a_file_shows_its_diff_and_one_on_the_list_leaves() {
    let mut world = world();
    opened(&mut world);
    let screen = render(&world.app);
    let (row, line) = screen
        .lines()
        .enumerate()
        .find(|(_, line)| line.contains("src/login.rs"))
        .unwrap();
    let column = u16::try_from(line.chars().position(|c| c == 'A').unwrap()).unwrap();
    let click = |column, row| MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    };
    let (_, asked) = command(
        &world
            .app
            .on_mouse(click(column, u16::try_from(row).unwrap())),
    );
    assert!(
        matches!(&asked, Command::FileDiff { path, .. } if path == "src/login.rs"),
        "{asked:?}"
    );
    world.app.on_mouse(click(2, 2));
    assert_eq!(world.app.mode, Mode::Browse);
}

#[test]
fn a_worktree_line_says_what_its_work_comes_to_beside_its_pull_request() {
    let mut world = world();
    upsert(&mut world, |worktree| {
        worktree.changes = Some(ChangeSummary {
            files: 5,
            added: 120,
            removed: 30,
        });
    });
    world.app.selected = Some(Row::Worktree(world.worktree.id));
    let screen = render(&world.app);
    let line = list_line(&screen, "⎇ work");
    assert!(line.contains("+120 −30"), "{line}");
    assert!(screen.contains("Changed +120 −30 in 5 files"), "{screen}");
    assert!(screen.contains("d changes"), "{screen}");

    // Short of room, the pull request's word goes, then what changed.
    upsert(&mut world, |worktree| {
        if let WorkspaceKind::Worktree { branch, .. } = &mut worktree.kind {
            "muster/issue-42-fix-the-login".clone_into(branch);
        }
        worktree.pull_request = Some(PullRequestStatus {
            number: 42,
            url: "https://github.com/o/app/pull/42".to_owned(),
            state: PullRequestState::Open,
            checks: None,
            review: None,
        });
    });
    let screen = render(&world.app);
    let line = list_line(&screen, "⎇ muster/");
    assert!(line.contains("#42"), "{line}");
    assert!(!line.contains("+120"), "{line}");
}

/// The list's part of the screen's line holding `text`, without the pane
/// beside it.
fn list_line(screen: &str, text: &str) -> String {
    let line = screen
        .lines()
        .find(|line| line.contains(text))
        .unwrap_or_else(|| panic!("no line with {text:?} in\n{screen}"));
    line.chars().take(34).collect()
}

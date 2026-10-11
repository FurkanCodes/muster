use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use muster_core::protocol::{
    ClientFrame, Command, Delta, ErrorBody, ErrorCode, Reply, ServerFrame, Snapshot, TermSize,
    TerminalModes,
};
use muster_core::{Project, Session, SessionStatus, WaitReason, Workspace, WorkspaceId};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use super::{Action, App, Dialog, HarnessChoice, Link, Mode, View};
use crate::model::Row;
use crate::model::tests::{project, session, worktree};

pub(super) const SIZE: TermSize = TermSize {
    cols: 100,
    rows: 29,
};

pub(super) struct World {
    pub(super) app: App,
    pub(super) project: Project,
    pub(super) worktree: Workspace,
    pub(super) session: Session,
}

/// An app connected to a daemon with one project, worktree and session.
pub(super) fn world() -> World {
    let mut app = App::new(PathBuf::from("/src"), SIZE);
    assert_eq!(
        app.on_connected("0.1.0".to_owned()),
        [Action::Send(ClientFrame::Subscribe)]
    );
    let project = project("app");
    let worktree = worktree(project.id, "work");
    let session = session(worktree.id, "Shell", 3);
    app.on_frame(ServerFrame::Snapshot(Snapshot {
        projects: vec![project.clone()],
        workspaces: vec![worktree.clone()],
        sessions: vec![session.clone()],
        ..Snapshot::default()
    }));
    World {
        app,
        project,
        worktree,
        session,
    }
}

pub(super) fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

pub(super) fn ctrl(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
}

pub(super) fn press(app: &mut App, code: KeyCode) -> Vec<Action> {
    app.on_key(key(code))
}

pub(super) fn typed(app: &mut App, text: &str) {
    for c in text.chars() {
        press(app, KeyCode::Char(c));
    }
}

/// The command in `actions`, with its request id.
pub(super) fn command(actions: &[Action]) -> (u64, Command) {
    actions
        .iter()
        .find_map(|action| match action {
            Action::Send(ClientFrame::Command { req_id, command }) => {
                Some((*req_id, command.clone()))
            }
            _ => None,
        })
        .expect("a command")
}

pub(super) fn render(app: &App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(SIZE.cols, SIZE.rows + 1)).unwrap();
    terminal
        .draw(|frame| crate::view::draw(frame, app))
        .unwrap();
    let buffer = terminal.backend().buffer();
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn the_list_shows_projects_worktrees_and_sessions() {
    let world = world();
    let screen = render(&world.app);
    assert!(screen.contains("app"), "{screen}");
    assert!(screen.contains("⎇ work"), "{screen}");
    assert!(screen.contains("Shell  idle"), "{screen}");
    assert!(screen.contains("daemon 0.1.0"), "{screen}");
}

#[test]
fn resource_use_is_shown_for_the_host_and_each_session() {
    let mut world = world();
    world.app.on_frame(ServerFrame::Delta(Delta::Resources(
        muster_core::protocol::ResourceReport {
            host_memory_used: 9_100_000_000,
            host_memory_total: 16_000_000_000,
            sessions: vec![muster_core::protocol::SessionUsage {
                session: world.session.id,
                memory_bytes: 312_400_000,
                cpu_permille: 41,
            }],
        },
    )));
    let screen = render(&world.app);
    assert!(screen.contains("memory 9.1 GB of 16.0 GB"), "{screen}");
    assert!(screen.contains("313 MB · 5%"), "{screen}");
}

#[test]
fn selecting_a_session_streams_it_and_moving_away_stops() {
    let mut world = world();
    assert_eq!(world.app.selected, Some(Row::Project(world.project.id)));
    press(&mut world.app, KeyCode::Down);
    let actions = press(&mut world.app, KeyCode::Down);
    let id = world.session.id;
    assert_eq!(
        actions,
        [
            Action::Send(ClientFrame::Resize {
                session: id,
                size: SIZE
            }),
            Action::Send(ClientFrame::Attach {
                session: id,
                from_seq: None
            }),
        ]
    );
    let actions = press(&mut world.app, KeyCode::Up);
    assert_eq!(actions, [Action::Send(ClientFrame::Detach { session: id })]);
}

#[test]
fn a_terminal_takes_keys_and_ctrl_bracket_leaves_it() {
    let mut world = world();
    let id = world.session.id;
    world.app.selected = Some(Row::Session(id));
    press(&mut world.app, KeyCode::Enter);
    assert_eq!(world.app.mode, Mode::Terminal(id));

    assert_eq!(
        press(&mut world.app, KeyCode::Char('q')),
        [Action::Send(ClientFrame::Input {
            session: id,
            data: b"q".to_vec()
        })]
    );
    world.app.on_key(ctrl(']'));
    assert_eq!(world.app.mode, Mode::Browse);

    press(&mut world.app, KeyCode::Enter);
    // Without the kitty protocol the terminal sends 0x1d, which crossterm
    // calls Ctrl+5.
    world.app.on_key(ctrl('5'));
    assert_eq!(world.app.mode, Mode::Browse);
}

#[test]
fn output_is_drawn_and_pastes_follow_the_programs_modes() {
    let mut world = world();
    let id = world.session.id;
    world.app.selected = Some(Row::Session(id));
    press(&mut world.app, KeyCode::Enter);
    world.app.on_frame(ServerFrame::Replay {
        session: id,
        seq: 0,
        data: b"$ echo hi\r\nhi\r\n".to_vec(),
        reset: true,
    });
    world.app.on_frame(ServerFrame::Output {
        session: id,
        seq: 15,
        data: b"$ ".to_vec(),
    });
    let screen = render(&world.app);
    assert!(screen.contains("$ echo hi"), "{screen}");
    assert!(screen.contains("Ctrl-] back"), "{screen}");

    assert_eq!(
        world.app.on_paste("a\nb"),
        [Action::Send(ClientFrame::Input {
            session: id,
            data: b"a\rb".to_vec()
        })]
    );
    world.app.on_frame(ServerFrame::Modes {
        session: id,
        modes: TerminalModes {
            bracketed_paste: true,
            ..TerminalModes::default()
        },
    });
    assert_eq!(
        world.app.on_paste("x"),
        [Action::Send(ClientFrame::Input {
            session: id,
            data: b"\x1b[200~x\x1b[201~".to_vec()
        })]
    );
}

#[test]
fn a_new_worktree_is_created_and_selected() {
    let mut world = world();
    press(&mut world.app, KeyCode::Char('w'));
    typed(&mut world.app, "fix-login");
    let (req_id, sent) = command(&press(&mut world.app, KeyCode::Enter));
    assert_eq!(
        sent,
        Command::CreateWorktree {
            project: world.project.id,
            branch: Some("fix-login".to_owned()),
        }
    );
    let created = worktree(world.project.id, "fix-login");
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::WorkspaceUpserted(
            created.clone(),
        )));
    world.app.on_frame(ServerFrame::Reply {
        req_id,
        outcome: Ok(Reply::WorktreeCreated { id: created.id }),
    });
    assert_eq!(world.app.selected, Some(Row::Worktree(created.id)));
}

#[test]
fn a_new_session_starts_claude_with_a_prompt_and_opens_it() {
    let mut world = world();
    world.app.selected = Some(Row::Worktree(world.worktree.id));
    press(&mut world.app, KeyCode::Char('n'));
    assert!(matches!(
        world.app.mode,
        Mode::Dialog(Dialog::NewSession {
            harness: HarnessChoice::Claude,
            ..
        })
    ));
    typed(&mut world.app, "fix the login");
    let (req_id, sent) = command(&press(&mut world.app, KeyCode::Enter));
    assert_eq!(
        sent,
        Command::StartSession {
            workspace: world.worktree.id,
            harness: "claude".to_owned(),
            title: None,
            size: SIZE,
            prompt: Some("fix the login".to_owned()),
        }
    );
    let started = session(world.worktree.id, "Claude Code", 4);
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::SessionUpserted(started.clone())));
    let actions = world.app.on_frame(ServerFrame::Reply {
        req_id,
        outcome: Ok(Reply::SessionStarted { id: started.id }),
    });
    assert_eq!(world.app.mode, Mode::Terminal(started.id));
    assert!(actions.contains(&Action::Send(ClientFrame::Attach {
        session: started.id,
        from_seq: None,
    })));
}

#[test]
fn a_reply_that_arrives_before_its_delta_still_selects_and_opens() {
    let mut world = world();
    press(&mut world.app, KeyCode::Char('w'));
    let (req_id, _) = command(&press(&mut world.app, KeyCode::Enter));
    let created = worktree(world.project.id, "early");
    world.app.on_frame(ServerFrame::Reply {
        req_id,
        outcome: Ok(Reply::WorktreeCreated { id: created.id }),
    });
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::WorkspaceUpserted(
            created.clone(),
        )));
    assert_eq!(world.app.selected, Some(Row::Worktree(created.id)));

    press(&mut world.app, KeyCode::Char('n'));
    let (req_id, _) = command(&press(&mut world.app, KeyCode::Enter));
    let started = session(created.id, "Claude Code", 5);
    world.app.on_frame(ServerFrame::Reply {
        req_id,
        outcome: Ok(Reply::SessionStarted { id: started.id }),
    });
    assert_eq!(world.app.mode, Mode::Browse, "not until it exists");
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::SessionUpserted(started.clone())));
    assert_eq!(world.app.mode, Mode::Terminal(started.id));
}

#[test]
fn tab_switches_a_new_session_to_a_shell_which_takes_no_prompt() {
    let mut world = world();
    world.app.selected = Some(Row::Worktree(world.worktree.id));
    press(&mut world.app, KeyCode::Char('n'));
    press(&mut world.app, KeyCode::Tab);
    typed(&mut world.app, "ignored");
    let (_, sent) = command(&press(&mut world.app, KeyCode::Enter));
    let Command::StartSession {
        harness, prompt, ..
    } = sent
    else {
        panic!("expected StartSession");
    };
    assert_eq!((harness.as_str(), prompt), ("shell", None));
}

#[test]
fn a_refused_removal_offers_to_force_it() {
    let mut world = world();
    world.app.selected = Some(Row::Worktree(world.worktree.id));
    press(&mut world.app, KeyCode::Char('x'));
    let (req_id, sent) = command(&press(&mut world.app, KeyCode::Char('y')));
    assert_eq!(
        sent,
        Command::RemoveWorktree {
            workspace: world.worktree.id,
            force: false
        }
    );
    world.app.on_frame(ServerFrame::Reply {
        req_id,
        outcome: Err(ErrorBody::new(
            ErrorCode::Conflict,
            "/w/work has uncommitted changes; remove it with force to discard them",
        )),
    });
    assert!(matches!(
        world.app.mode,
        Mode::Dialog(Dialog::Confirm { .. })
    ));
    let (_, forced) = command(&press(&mut world.app, KeyCode::Char('y')));
    assert_eq!(
        forced,
        Command::RemoveWorktree {
            workspace: world.worktree.id,
            force: true
        }
    );
}

#[test]
fn other_errors_are_shown() {
    let mut world = world();
    press(&mut world.app, KeyCode::Char('w'));
    let (req_id, _) = command(&press(&mut world.app, KeyCode::Enter));
    world.app.on_frame(ServerFrame::Reply {
        req_id,
        outcome: Err(ErrorBody::new(
            ErrorCode::BadRequest,
            "\"bad..name\" is not a valid branch name",
        )),
    });
    let note = world.app.note.clone().unwrap();
    assert!(note.error && note.text.contains("not a valid branch name"));
    assert!(render(&world.app).contains("not a valid branch name"));
}

#[test]
fn stopping_asks_first() {
    let mut world = world();
    world.app.selected = Some(Row::Session(world.session.id));
    press(&mut world.app, KeyCode::Char('s'));
    assert!(press(&mut world.app, KeyCode::Char('n')).is_empty());
    press(&mut world.app, KeyCode::Char('s'));
    let (_, sent) = command(&press(&mut world.app, KeyCode::Char('y')));
    assert_eq!(
        sent,
        Command::StopSession {
            session: world.session.id
        }
    );
}

#[test]
fn needs_you_holds_its_order_while_a_session_is_looked_at() {
    let mut world = world();
    let other = session(world.worktree.id, "Claude Code", 4);
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::SessionUpserted(other.clone())));
    for (id, status) in [
        (world.session.id, SessionStatus::Finished),
        (
            other.id,
            SessionStatus::NeedsYou {
                reason: WaitReason::Question,
            },
        ),
    ] {
        world.app.on_frame(ServerFrame::Delta(Delta::SessionStatus {
            session: id,
            status,
            unseen: true,
        }));
    }
    press(&mut world.app, KeyCode::Tab);
    assert_eq!(world.app.view, View::NeedsYou);
    assert_eq!(world.app.needs_list, [other.id, world.session.id]);
    assert_eq!(world.app.needs_selected, Some(other.id));

    // Looking at it clears it, yet it stays until the selection moves on.
    world.app.on_frame(ServerFrame::Delta(Delta::SessionStatus {
        session: other.id,
        status: SessionStatus::Running,
        unseen: false,
    }));
    assert_eq!(world.app.needs_list, [other.id, world.session.id]);
    press(&mut world.app, KeyCode::Down);
    assert_eq!(world.app.needs_list, [world.session.id]);
    assert!(render(&world.app).contains("1 need you"));
}

#[test]
fn losing_the_daemon_is_shown_and_nothing_streams() {
    let mut world = world();
    world
        .app
        .on_disconnected("the daemon closed the connection".to_owned());
    assert!(matches!(world.app.link, Link::Lost(_)));
    assert!(render(&world.app).contains("retrying"));
    world.app.selected = Some(Row::Session(world.session.id));
    assert!(press(&mut world.app, KeyCode::Down).is_empty());
}

#[test]
fn a_project_is_added_from_a_typed_path() {
    let mut world = world();
    press(&mut world.app, KeyCode::Char('a'));
    let Mode::Dialog(Dialog::AddProject { path }) = &world.app.mode else {
        panic!("expected the add-project dialog");
    };
    assert_eq!(path.text(), "/src", "starts with where muster was started");
    world.app.on_key(ctrl('u'));
    typed(&mut world.app, "other");
    let (_, sent) = command(&press(&mut world.app, KeyCode::Enter));
    assert_eq!(
        sent,
        Command::AddProject {
            root: PathBuf::from("/src/other"),
            name: None
        }
    );
    let _ = WorkspaceId::new();
}

#[test]
fn issues_load_into_a_picker_and_start_work_that_opens() {
    let mut world = world();
    let actions = press(&mut world.app, KeyCode::Char('i'));
    let (req_id, sent) = command(&actions);
    assert_eq!(
        sent,
        Command::ListIssues {
            project: world.project.id
        }
    );
    assert!(render(&world.app).contains("Loading…"));

    let issue = |number: u64, title: &str| muster_core::Issue {
        number,
        title: title.to_owned(),
        url: format!("https://github.com/o/r/issues/{number}"),
        labels: vec!["bug".to_owned()],
        author: None,
    };
    world.app.on_frame(ServerFrame::Reply {
        req_id,
        outcome: Ok(Reply::Issues {
            issues: vec![issue(42, "Fix the login redirect"), issue(7, "Dark mode")],
        }),
    });
    let screen = render(&world.app);
    assert!(screen.contains("#42"), "{screen}");
    assert!(screen.contains("Fix the login redirect"), "{screen}");

    press(&mut world.app, KeyCode::Down);
    let (req_id, sent) = command(&press(&mut world.app, KeyCode::Enter));
    assert_eq!(
        sent,
        Command::StartFromIssue {
            project: world.project.id,
            number: 7,
            harness: "claude".to_owned(),
            size: SIZE,
        }
    );
    let started = session(world.worktree.id, "#7 Dark mode", 9);
    world.app.on_frame(ServerFrame::Reply {
        req_id,
        outcome: Ok(Reply::IssueStarted {
            workspace: world.worktree.id,
            session: started.id,
        }),
    });
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::SessionUpserted(started.clone())));
    assert_eq!(world.app.mode, Mode::Terminal(started.id));
}

#[test]
fn an_issue_listing_error_is_shown_in_the_picker() {
    let mut world = world();
    let (req_id, _) = command(&press(&mut world.app, KeyCode::Char('i')));
    world.app.on_frame(ServerFrame::Reply {
        req_id,
        outcome: Err(ErrorBody::new(
            ErrorCode::BadRequest,
            "the GitHub CLI (gh) is not installed; install it and run `gh auth login`",
        )),
    });
    assert!(render(&world.app).contains("not installed"));
    press(&mut world.app, KeyCode::Esc);
    assert_eq!(world.app.mode, Mode::Browse);
}

#[test]
fn a_long_error_wraps_instead_of_being_cut_off() {
    let mut world = world();
    world.app.complain(
        "`git worktree add -b test /Users/me/.muster/worktrees/lamphaus/test` failed: \
         fatal: the reason that must stay readable at the end of a long line\nsecond line",
    );
    let screen = render(&world.app);
    assert!(screen.contains("must stay readable"), "{screen}");
    assert!(screen.contains("· second line"), "{screen}");
}

#[test]
fn ctrl_z_is_held_back_from_claude_but_reaches_a_shell() {
    let mut world = world();
    let claude = Session {
        harness: "claude".to_owned(),
        ..session(world.worktree.id, "Claude Code", 7)
    };
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::SessionUpserted(claude.clone())));

    world.app.selected = Some(Row::Session(claude.id));
    press(&mut world.app, KeyCode::Enter);
    assert!(world.app.on_key(ctrl('z')).is_empty());
    assert!(render(&world.app).contains("Ctrl+Z would freeze Claude Code"));
    // The next key clears the message.
    press(&mut world.app, KeyCode::Char('a'));
    assert!(world.app.note.is_none());

    world.app.on_key(ctrl(']'));
    world.app.selected = Some(Row::Session(world.session.id));
    press(&mut world.app, KeyCode::Enter);
    assert_eq!(
        world.app.on_key(ctrl('z')),
        [Action::Send(ClientFrame::Input {
            session: world.session.id,
            data: vec![0x1a]
        })]
    );
}

#[test]
fn before_the_first_snapshot_the_ui_says_it_is_loading() {
    let mut app = App::new(PathBuf::from("/src"), SIZE);
    app.on_connected("0.1.0".to_owned());
    let screen = render(&app);
    assert!(screen.contains("loading…"), "{screen}");
    assert!(!screen.contains("0 sessions"), "{screen}");
}

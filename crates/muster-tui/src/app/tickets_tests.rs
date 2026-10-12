//! The Tickets screen: fetching each project's tickets and the selected
//! one's text, the agents on a ticket, and assigning more.

use std::path::PathBuf;

use crossterm::event::KeyCode;
use muster_core::protocol::{
    ClientFrame, Command, Delta, ErrorBody, ErrorCode, Reply, ServerFrame,
};
use muster_core::{
    Issue, SessionStatus, TicketKey, Timestamp, Workspace, WorkspaceId, WorkspaceKind,
};

use super::tests::{PANE, World, command, press, render, world};
use super::{Action, Mode, TicketRow, View};
use crate::model::tests::session;

fn key(number: u64) -> TicketKey {
    TicketKey::number(number)
}

fn issue(number: u64, title: &str) -> Issue {
    Issue {
        key: key(number),
        title: title.to_owned(),
        url: format!("https://github.com/o/app/issues/{number}"),
        labels: vec!["bug".to_owned()],
        author: Some("someone".to_owned()),
    }
}

fn commands(actions: &[Action]) -> Vec<Command> {
    actions
        .iter()
        .filter_map(|action| match action {
            Action::Send(ClientFrame::Command { command, .. }) => Some(command.clone()),
            _ => None,
        })
        .collect()
}

fn reply(world: &mut World, req_id: u64, outcome: Result<Reply, ErrorBody>) -> Vec<Action> {
    world.app.on_frame(ServerFrame::Reply { req_id, outcome })
}

/// The world on the Tickets screen with #42 and #7 fetched, #42 selected
/// and its text in.
fn on_tickets() -> World {
    let mut world = world();
    let actions = press(&mut world.app, KeyCode::Char('i'));
    assert_eq!(world.app.view, View::Tickets);
    let (req_id, listed) = command(&actions);
    assert_eq!(
        listed,
        Command::ListIssues {
            project: world.project.id
        }
    );
    assert!(render(&world.app).contains("app  fetching…"));
    reply(
        &mut world,
        req_id,
        Ok(Reply::Issues {
            tracker: "GitHub issues".to_owned(),
            issues: vec![issue(42, "Fix the login redirect"), issue(7, "Dark mode")],
        }),
    );
    assert_eq!(
        world.app.ticket_rows(),
        [
            TicketRow::Project(world.project.id),
            TicketRow::Ticket(world.project.id, key(42)),
            TicketRow::Ticket(world.project.id, key(7)),
        ]
    );
    let actions = press(&mut world.app, KeyCode::Down);
    let (req_id, viewed) = command(&actions);
    assert_eq!(
        viewed,
        Command::ViewIssue {
            project: world.project.id,
            key: key(42)
        }
    );
    reply(
        &mut world,
        req_id,
        Ok(Reply::IssueViewed {
            issue: issue(42, "Fix the login redirect"),
            body: "After signing in, users land on /404.".to_owned(),
        }),
    );
    world
}

/// The worktree an agent on #42 works in, and the agent, as the state
/// shows them.
fn agent_on_42(world: &mut World, branch: &str, created: i64) -> muster_core::Session {
    let (workspace, agent) = agent_for_42(world, branch, created);
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::WorkspaceUpserted(workspace)));
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::SessionUpserted(agent.clone())));
    agent
}

fn agent_for_42(world: &World, branch: &str, created: i64) -> (Workspace, muster_core::Session) {
    let workspace = Workspace {
        id: WorkspaceId::new(),
        project: world.project.id,
        kind: WorkspaceKind::Worktree {
            path: PathBuf::from(format!("/w/{branch}")),
            branch: branch.to_owned(),
        },
        created_at: Timestamp::from_millis(created),
    };
    let mut agent = session(workspace.id, "#42 Fix the login redirect", created);
    agent.harness = "claude".to_owned();
    agent.status = SessionStatus::Running;
    (workspace, agent)
}

#[test]
fn a_project_s_tickets_are_listed_and_the_selected_one_shown_with_its_text() {
    let world = on_tickets();
    let screen = render(&world.app);
    for shown in [
        "app  2 open",
        "#42 Fix the login redirect",
        "#7 Dark mode",
        "#42 · app · https://github.com/o/app/issues/42",
        "by someone · bug",
        "No agent on it yet",
        "After signing in, users land on /404.",
        "⏎ start an agent  b browser  x close  r refresh  ⇥ next tab  esc back",
    ] {
        assert!(screen.contains(shown), "{shown:?} in\n{screen}");
    }
    // The text is fetched once, not again on coming back to it.
    let mut world = world;
    press(&mut world.app, KeyCode::Down);
    let actions = press(&mut world.app, KeyCode::Up);
    assert!(commands(&actions).is_empty(), "{actions:?}");
}

#[test]
fn enter_starts_the_first_agent_then_opens_it_and_a_starts_another() {
    let mut world = on_tickets();
    let actions = press(&mut world.app, KeyCode::Enter);
    let (req_id, started) = command(&actions);
    assert_eq!(
        started,
        Command::StartFromIssue {
            project: world.project.id,
            key: key(42),
            harness: "claude".to_owned(),
            size: PANE,
            another: false,
        }
    );
    let (workspace, agent) = agent_for_42(&world, "muster/issue-42-fix-the-login-redirect", 10);
    reply(
        &mut world,
        req_id,
        Ok(Reply::IssueStarted {
            workspace: workspace.id,
            session: agent.id,
        }),
    );
    // The reply named a session the state has yet to show; once it does,
    // it opens, and coming back is coming back to the tickets.
    assert_eq!(world.app.mode, Mode::Browse);
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::WorkspaceUpserted(workspace)));
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::SessionUpserted(agent.clone())));
    assert_eq!(world.app.mode, Mode::Terminal(agent.id));
    world.app.mode = Mode::Browse;
    assert_eq!(world.app.view, View::Tickets);

    // With an agent on it, ⏎ opens that agent rather than starting one.
    let screen = render(&world.app);
    // Its agent is running: its mark spins (step 0 here).
    assert!(screen.contains("#42 Fix the login redirect ⠋"), "{screen}");
    assert!(screen.contains("Agents"), "{screen}");
    assert!(screen.contains("⏎ open agent  a another agent"), "{screen}");
    let actions = press(&mut world.app, KeyCode::Enter);
    assert!(commands(&actions).is_empty());
    assert_eq!(world.app.mode, Mode::Terminal(agent.id));
    world.app.mode = Mode::Browse;

    let actions = press(&mut world.app, KeyCode::Char('a'));
    assert!(matches!(
        &commands(&actions)[..],
        [Command::StartFromIssue {
            key: started,
            another: true,
            ..
        }] if *started == key(42)
    ));
    // The latest agent still running is the one ⏎ opens.
    let second = agent_on_42(&mut world, "muster/issue-42-fix-the-login-redirect-2", 20);
    world.app.mode = Mode::Browse;
    press(&mut world.app, KeyCode::Enter);
    assert_eq!(world.app.mode, Mode::Terminal(second.id));
}

#[test]
fn the_agents_on_a_ticket_are_those_on_branches_named_for_it() {
    let mut world = on_tickets();
    let first = agent_on_42(&mut world, "muster/issue-42-fix", 10);
    let second = agent_on_42(&mut world, "muster/issue-42-fix-2", 20);
    agent_on_42(&mut world, "muster/issue-4-other", 30);
    agent_on_42(&mut world, "feature/issue-42", 40);
    let agents: Vec<_> = world
        .app
        .model
        .agents_on(world.project.id, &key(42))
        .iter()
        .map(|session| session.id)
        .collect();
    assert_eq!(agents, [first.id, second.id]);
}

#[test]
fn a_failed_fetch_says_why_and_r_fetches_everything_again() {
    let mut world = world();
    let actions = press(&mut world.app, KeyCode::Char('i'));
    let (req_id, _) = command(&actions);
    reply(
        &mut world,
        req_id,
        Err(ErrorBody::new(
            ErrorCode::BadRequest,
            "the GitHub CLI (gh) is not signed in; run `gh auth login`",
        )),
    );
    let screen = render(&world.app);
    assert!(screen.contains("not signed in"), "{screen}");
    assert!(
        screen.contains("Its tickets could not be fetched"),
        "{screen}"
    );
    let actions = press(&mut world.app, KeyCode::Char('r'));
    assert_eq!(
        commands(&actions),
        [Command::ListIssues {
            project: world.project.id
        }]
    );
}

#[test]
fn b_opens_the_ticket_in_the_browser_and_other_screens_keys_do_nothing_here() {
    let mut world = on_tickets();
    assert_eq!(
        press(&mut world.app, KeyCode::Char('b')),
        [Action::Open(
            "https://github.com/o/app/issues/42".to_owned()
        )]
    );
    for key in ['s', 'n', 'w', 'c', 'v', 'o'] {
        let actions = press(&mut world.app, KeyCode::Char(key));
        assert!(commands(&actions).is_empty(), "{key}: {actions:?}");
        assert_eq!(world.app.mode, Mode::Browse, "{key}");
    }
}

#[test]
fn tab_goes_from_the_tickets_screen_round_to_sessions_and_back() {
    let mut world = on_tickets();
    press(&mut world.app, KeyCode::Tab);
    assert_eq!(world.app.view, View::Sessions);
    press(&mut world.app, KeyCode::BackTab);
    assert_eq!(world.app.view, View::Tickets);
}

#[test]
fn a_fetch_lost_with_the_daemon_is_asked_again() {
    let mut world = world();
    press(&mut world.app, KeyCode::Char('i'));
    world.app.on_disconnected("gone".to_owned());
    let actions = world.app.on_connected("0.1.0".to_owned());
    assert_eq!(actions, [Action::Send(ClientFrame::Subscribe)]);
    let actions = world
        .app
        .on_frame(ServerFrame::Snapshot(muster_core::protocol::Snapshot {
            projects: vec![world.project.clone()],
            workspaces: vec![world.worktree.clone()],
            sessions: vec![world.session.clone()],
            ..Default::default()
        }));
    assert_eq!(
        commands(&actions),
        [Command::ListIssues {
            project: world.project.id
        }]
    );
}

#[test]
fn x_closes_a_ticket_on_github_after_asking_and_it_leaves_the_list() {
    let mut world = on_tickets();
    assert!(commands(&press(&mut world.app, KeyCode::Char('x'))).is_empty());
    let screen = render(&world.app);
    assert!(
        screen.contains("Close #42 Fix the login redirect in GitHub issues?"),
        "{screen}"
    );
    let (req_id, sent) = command(&press(&mut world.app, KeyCode::Char('y')));
    assert_eq!(
        sent,
        Command::CloseIssue {
            project: world.project.id,
            key: key(42)
        }
    );
    reply(&mut world, req_id, Ok(Reply::Done));
    // No work on it, so nothing to clean up after.
    assert_eq!(world.app.mode, Mode::Browse);
    let screen = render(&world.app);
    assert!(!screen.contains("#42 Fix the login redirect"), "{screen}");
    assert!(screen.contains("Closed #42 in GitHub issues."), "{screen}");
    assert!(screen.contains("#7 Dark mode"), "{screen}");
}

#[test]
fn a_ticket_github_would_not_close_stays() {
    let mut world = on_tickets();
    press(&mut world.app, KeyCode::Char('x'));
    let (req_id, _) = command(&press(&mut world.app, KeyCode::Char('y')));
    reply(
        &mut world,
        req_id,
        Err(ErrorBody::new(ErrorCode::BadRequest, "gh: HTTP 403")),
    );
    let screen = render(&world.app);
    assert!(screen.contains("HTTP 403"), "{screen}");
    assert!(screen.contains("#42 Fix the login redirect"), "{screen}");
}

#[test]
fn after_closing_a_ticket_its_agents_and_worktrees_go_if_asked() {
    let mut world = on_tickets();
    let (workspace, agent) = agent_for_42(&world, "muster/issue-42-fix-the-login-redirect", 10);
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::WorkspaceUpserted(
            workspace.clone(),
        )));
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::SessionUpserted(agent.clone())));
    press(&mut world.app, KeyCode::Char('x'));
    let (req_id, _) = command(&press(&mut world.app, KeyCode::Char('y')));
    reply(&mut world, req_id, Ok(Reply::Done));
    let screen = render(&world.app);
    assert!(
        screen.contains("Closed #42 in GitHub issues. Also close its agent and remove its"),
        "{screen}"
    );
    assert!(screen.contains("worktree? Branches are kept."), "{screen}");

    // The agent is stopped; the worktree goes once nothing runs in it.
    let actions = press(&mut world.app, KeyCode::Char('y'));
    assert_eq!(
        commands(&actions),
        [Command::StopSession { session: agent.id }]
    );
    let mut ended = agent.clone();
    ended.status = SessionStatus::Exited { success: true };
    ended.ended_at = Some(Timestamp::from_millis(20));
    let actions = world
        .app
        .on_frame(ServerFrame::Delta(Delta::SessionUpserted(ended)));
    assert_eq!(
        commands(&actions),
        [Command::RemoveWorktree {
            workspace: workspace.id,
            force: false
        }]
    );
}

#[test]
fn declining_the_clean_up_leaves_the_work_alone() {
    let mut world = on_tickets();
    let (workspace, agent) = agent_for_42(&world, "muster/issue-42-fix-the-login-redirect", 10);
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::WorkspaceUpserted(workspace)));
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::SessionUpserted(agent)));
    press(&mut world.app, KeyCode::Char('x'));
    let (req_id, _) = command(&press(&mut world.app, KeyCode::Char('y')));
    reply(&mut world, req_id, Ok(Reply::Done));
    assert!(commands(&press(&mut world.app, KeyCode::Char('n'))).is_empty());
    assert_eq!(world.app.mode, Mode::Browse);
}

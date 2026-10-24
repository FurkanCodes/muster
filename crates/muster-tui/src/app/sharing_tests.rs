//! Projects sharing their tickets: two repositories whose tickets are in
//! one Jira project are listed once, the agents in either are on a ticket,
//! and an agent assigned to one starts in the project the person chooses,
//! the one last chosen offered first.

use std::path::PathBuf;

use crossterm::event::KeyCode;
use muster_core::protocol::{ClientFrame, Command, Delta, Reply, ServerFrame, Snapshot};
use muster_core::{Issue, Project, SessionStatus, TicketKey};

use super::tests::{PANE, SIZE, press, render};
use super::{Action, App, Confirmed, Dialog, HarnessChoice, Mode, TicketRow, View};
use crate::model::tests::{project, session, worktree};

const JIRA: &str = "jira https://acme.atlassian.net project = \"APP\" AND statusCategory != Done";

fn key(text: &str) -> TicketKey {
    TicketKey::parse(text).unwrap()
}

fn ticket(text: &str, title: &str) -> Issue {
    Issue {
        key: key(text),
        title: title.to_owned(),
        url: format!("https://acme.atlassian.net/browse/{text}"),
        labels: Vec::new(),
        author: None,
    }
}

struct Shared {
    app: App,
    /// `web` and `api` list APP's tickets; `docs` its own GitHub issues.
    web: Project,
    api: Project,
    docs: Project,
}

/// Three projects on the Tickets screen, their tickets fetched: `web` and
/// `api` share APP-7 and APP-9, `docs` has #1.
fn shared() -> Shared {
    let mut app = App::new(PathBuf::from("/src"), SIZE);
    app.on_connected("0.1.0".to_owned());
    let (web, api, docs) = (project("web"), project("api"), project("docs"));
    app.on_frame(ServerFrame::Snapshot(Snapshot {
        projects: vec![web.clone(), api.clone(), docs.clone()],
        ..Snapshot::default()
    }));
    let actions = press(&mut app, KeyCode::Char('i'));
    assert_eq!(app.view, View::Tickets);
    for action in actions {
        let Action::Send(ClientFrame::Command {
            req_id,
            command: Command::ListIssues { project },
        }) = action
        else {
            continue;
        };
        let reply = if project == docs.id {
            Reply::Issues {
                issues: vec![Issue {
                    key: TicketKey::number(1),
                    title: "Typo".to_owned(),
                    url: "https://github.com/o/docs/issues/1".to_owned(),
                    labels: Vec::new(),
                    author: None,
                }],
                tracker: "GitHub issues".to_owned(),
                mine: false,
                source: None,
            }
        } else {
            Reply::Issues {
                issues: vec![
                    ticket("APP-7", "Fix the login redirect"),
                    ticket("APP-9", "Dark mode"),
                ],
                tracker: "Jira (acme.atlassian.net)".to_owned(),
                mine: false,
                source: Some(JIRA.to_owned()),
            }
        };
        app.on_frame(ServerFrame::Reply {
            req_id,
            outcome: Ok(reply),
        });
    }
    Shared {
        app,
        web,
        api,
        docs,
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

/// An agent on `branch` in `project`, as the daemon would tell.
fn agent(app: &mut App, project: &Project, branch: &str, created: i64) {
    let workspace = worktree(project.id, branch);
    let mut agent = session(workspace.id, "APP-7 Fix the login redirect", created);
    agent.harness = "claude".to_owned();
    agent.status = SessionStatus::Running;
    app.on_frame(ServerFrame::Delta(Delta::WorkspaceUpserted(workspace)));
    app.on_frame(ServerFrame::Delta(Delta::SessionUpserted(agent)));
    app.mode = Mode::Browse;
}

/// Selects `row` on the Tickets screen.
fn select(app: &mut App, row: &TicketRow) {
    while app.tickets_selected.as_ref() != Some(row) {
        press(app, KeyCode::Down);
    }
}

fn into(app: &App) -> muster_core::ProjectId {
    match &app.mode {
        Mode::Dialog(Dialog::AssignAgent { into, .. }) => *into,
        other => panic!("expected the assign dialog, got {other:?}"),
    }
}

#[test]
fn projects_listing_the_same_tickets_have_one_heading_and_each_ticket_once() {
    let Shared {
        app,
        web,
        api,
        docs,
        ..
    } = shared();
    assert_eq!(
        app.ticket_rows(),
        [
            TicketRow::Project(web.id),
            TicketRow::Ticket(web.id, key("APP-7")),
            TicketRow::Ticket(web.id, key("APP-9")),
            TicketRow::Project(docs.id),
            TicketRow::Ticket(docs.id, TicketKey::number(1)),
        ]
    );
    assert_eq!(app.sharing(api.id), [web.id, api.id]);
    assert_eq!(
        app.sharing(docs.id),
        [docs.id],
        "its own issues are its own"
    );
    let screen = render(&app);
    assert!(screen.contains("web · api  2 open"), "{screen}");
    assert!(
        screen.contains("these projects list the same ones"),
        "{screen}"
    );
}

#[test]
fn an_agent_on_a_shared_ticket_starts_in_the_project_chosen_offered_first_next_time() {
    let Shared {
        mut app, web, api, ..
    } = shared();
    select(&mut app, &TicketRow::Ticket(web.id, key("APP-7")));
    press(&mut app, KeyCode::Enter);
    assert_eq!(into(&app), web.id, "the first is offered first");
    let screen = render(&app);
    for shown in ["Which project is it for?", " web ", " api ", "↑↓ project"] {
        assert!(screen.contains(shown), "{shown:?} in\n{screen}");
    }
    press(&mut app, KeyCode::Down);
    assert_eq!(into(&app), api.id);
    press(&mut app, KeyCode::Down);
    assert_eq!(into(&app), web.id, "round the end");
    press(&mut app, KeyCode::Up);
    let actions = press(&mut app, KeyCode::Enter);
    assert_eq!(
        commands(&actions),
        [Command::StartFromIssue {
            project: api.id,
            key: key("APP-7"),
            harness: HarnessChoice::CLAUDE.id().to_owned(),
            size: PANE,
            another: false,
            on: None,
            brief: None,
        }]
    );

    // The agent in api is on the ticket, named for where it works.
    agent(
        &mut app,
        &api,
        "muster/issue-APP-7-fix-the-login-redirect",
        10,
    );
    assert_eq!(app.ticket_agents(web.id, &key("APP-7")).len(), 1);
    let screen = render(&app);
    assert!(screen.contains("Claude Code  running  api"), "{screen}");

    // Next time api is offered first, and esc starts nothing.
    select(&mut app, &TicketRow::Ticket(web.id, key("APP-9")));
    press(&mut app, KeyCode::Enter);
    assert_eq!(into(&app), api.id);
    assert!(press(&mut app, KeyCode::Esc).is_empty());
    assert_eq!(app.mode, Mode::Browse);
}

#[test]
fn another_agent_starts_in_a_worktree_of_its_own_only_where_the_ticket_has_one() {
    let Shared {
        mut app, web, api, ..
    } = shared();
    agent(
        &mut app,
        &web,
        "muster/issue-APP-7-fix-the-login-redirect",
        10,
    );
    select(&mut app, &TicketRow::Ticket(web.id, key("APP-7")));
    press(&mut app, KeyCode::Char('a'));
    assert!(render(&app).contains("In a worktree of its own"));
    let actions = press(&mut app, KeyCode::Enter);
    assert!(matches!(
        &commands(&actions)[..],
        [Command::StartFromIssue { project, another: true, .. }] if *project == web.id
    ));

    // In api, where nothing works on it yet, it is the ticket's first.
    press(&mut app, KeyCode::Char('a'));
    press(&mut app, KeyCode::Down);
    assert!(render(&app).contains("In the ticket's worktree"));
    let actions = press(&mut app, KeyCode::Enter);
    assert!(matches!(
        &commands(&actions)[..],
        [Command::StartFromIssue { project, another: false, .. }] if *project == api.id
    ));
}

#[test]
fn a_shared_ticket_closed_leaves_every_listing_and_its_work_everywhere_is_offered_to_clean_up() {
    let Shared {
        mut app, web, api, ..
    } = shared();
    agent(
        &mut app,
        &web,
        "muster/issue-APP-7-fix-the-login-redirect",
        10,
    );
    agent(
        &mut app,
        &api,
        "muster/issue-APP-7-fix-the-login-redirect",
        20,
    );
    select(&mut app, &TicketRow::Ticket(web.id, key("APP-7")));
    // ⏎ opens the latest agent, wherever it works.
    press(&mut app, KeyCode::Enter);
    assert!(matches!(app.mode, Mode::Terminal(_)));
    app.mode = Mode::Browse;

    press(&mut app, KeyCode::Char('x'));
    let actions = press(&mut app, KeyCode::Char('y'));
    let Some(Action::Send(ClientFrame::Command { req_id, command })) = actions.first() else {
        panic!("{actions:?}");
    };
    assert_eq!(
        *command,
        Command::CloseIssue {
            project: web.id,
            key: key("APP-7")
        }
    );
    app.on_frame(ServerFrame::Reply {
        req_id: *req_id,
        outcome: Ok(Reply::Done),
    });
    assert!(app.ticket(web.id, &key("APP-7")).is_none());
    assert!(app.ticket(api.id, &key("APP-7")).is_none());
    let Mode::Dialog(Dialog::Confirm { question, then }) = &app.mode else {
        panic!("{:?}", app.mode);
    };
    assert!(
        question.contains("close its 2 agents and remove its 2 worktrees"),
        "{question}"
    );
    assert_eq!(
        *then,
        Confirmed::CleanUpTicket {
            project: web.id,
            key: key("APP-7")
        }
    );
    let actions = press(&mut app, KeyCode::Char('y'));
    assert_eq!(
        commands(&actions)
            .iter()
            .filter(|command| matches!(command, Command::StopSession { .. }))
            .count(),
        2
    );
}

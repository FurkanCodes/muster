//! Runs in the UI: the runs list, a run's details and timeline, the
//! person's commands on it, runs in Needs you, and handing an issue to the
//! autopilot.

use crossterm::event::KeyCode;
use muster_core::protocol::{Command, Delta, ErrorBody, ErrorCode, Reply, ServerFrame};
use muster_core::{
    ProjectId, PullRequest, Run, RunId, RunStage, RunTransition, Ticket, Timestamp, Workspace,
    WorkspaceId, WorkspaceKind,
};

use super::tests::{World, command, press, render, typed, world};
use super::{Action, Dialog, IssueWork, Mode, View};
use crate::model::Need;
use crate::model::tests::session;

const MINUTE: i64 = 60_000;

fn run(project: ProjectId, number: u64, stage: RunStage) -> Run {
    Run {
        id: RunId::new(),
        project,
        ticket: Ticket {
            number,
            title: format!("Issue {number}"),
            url: format!("https://github.com/o/app/issues/{number}"),
            body: String::new(),
            labels: Vec::new(),
            author: None,
        },
        branch: format!("muster/issue-{number}-issue"),
        stage,
        attempt: 1,
        max_attempts: 3,
        reason: None,
        pr: None,
        cost_micros: None,
        created_at: Timestamp::from_millis(number as i64 * MINUTE),
        ended_at: stage
            .is_over()
            .then_some(Timestamp::from_millis(100 * MINUTE)),
    }
}

fn upsert(world: &mut World, run: &Run) -> Vec<Action> {
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::RunUpserted(run.clone())))
}

/// The world with three runs: #7 building, #42 needing the person, #3
/// merged; shown in the runs view.
fn with_runs() -> (World, Run, Run, Run) {
    let mut world = world();
    let building = run(world.project.id, 7, RunStage::Building);
    let mut stopped = run(world.project.id, 42, RunStage::NeedsYou);
    stopped.attempt = 3;
    stopped.reason = Some("the checks failed after 3 attempt(s)".to_owned());
    let merged = run(world.project.id, 3, RunStage::Merged);
    for each in [&building, &stopped, &merged] {
        upsert(&mut world, each);
    }
    world.app.now = Timestamp::from_millis(70 * MINUTE);
    (world, building, stopped, merged)
}

fn to_runs(world: &mut World) -> Vec<Action> {
    let mut actions = press(&mut world.app, KeyCode::Tab);
    actions.extend(press(&mut world.app, KeyCode::Tab));
    assert_eq!(world.app.view, View::Runs);
    actions
}

fn reply(world: &mut World, req_id: u64, outcome: Result<Reply, ErrorBody>) -> Vec<Action> {
    world.app.on_frame(ServerFrame::Reply { req_id, outcome })
}

#[test]
fn the_runs_view_puts_what_waits_on_you_first_and_shows_the_selected_run_s_timeline() {
    let (mut world, building, stopped, merged) = with_runs();
    let actions = to_runs(&mut world);
    assert_eq!(world.app.runs_list, [stopped.id, building.id, merged.id]);
    assert_eq!(world.app.runs_selected, Some(stopped.id));

    // Its timeline is asked for as it is shown, and drawn when it comes.
    let (req_id, asked) = command(&actions);
    assert_eq!(asked, Command::RunHistory { run: stopped.id });
    let step = |to, minutes, reason: &str| RunTransition {
        from: RunStage::Queued,
        to,
        reason: reason.to_owned(),
        at: Timestamp::from_millis(42 * MINUTE + minutes * MINUTE),
    };
    let transitions = vec![
        step(RunStage::Provisioning, 0, "there is room to run"),
        step(RunStage::Building, 1, "its container is ready"),
        step(
            RunStage::NeedsYou,
            20,
            "the checks failed after 3 attempt(s)",
        ),
    ];
    reply(&mut world, req_id, Ok(Reply::RunHistory { transitions }));
    let screen = render(&world.app);
    for shown in [
        "▲ #42 needs you  Issue 42",
        "● #7 building  Issue 7",
        "✓ #3 merged  Issue 3",
        "Stage    needs you: the checks failed after 3 attempt(s)",
        "Attempt  3 of 3",
        "Time     28 min",
        "+1 min",
        "its container is ready",
        "t take over  r retry  x cancel  n new run",
    ] {
        assert!(screen.contains(shown), "{shown:?} in\n{screen}");
    }
    // #7 and #42 are going; #3 has ended.
    assert!(screen.contains("2 runs going"), "{screen}");

    // Moving on asks for the next run's timeline; coming back, not again
    // unless the run moved.
    let actions = press(&mut world.app, KeyCode::Down);
    assert_eq!(
        command(&actions).1,
        Command::RunHistory { run: building.id }
    );
    let actions = press(&mut world.app, KeyCode::Up);
    assert_eq!(command(&actions).1, Command::RunHistory { run: stopped.id });
    let mut moved = stopped.clone();
    moved.stage = RunStage::Provisioning;
    let actions = upsert(&mut world, &moved);
    assert_eq!(command(&actions).1, Command::RunHistory { run: stopped.id });
}

#[test]
fn the_person_s_commands_go_to_the_run_in_focus_when_they_apply() {
    let (mut world, building, stopped, _) = with_runs();
    to_runs(&mut world);

    // Retry, with notes for its agent.
    press(&mut world.app, KeyCode::Char('r'));
    assert!(matches!(
        world.app.mode,
        Mode::Dialog(Dialog::RunNotes { .. })
    ));
    typed(&mut world.app, "use the helper");
    let actions = press(&mut world.app, KeyCode::Enter);
    assert_eq!(
        command(&actions).1,
        Command::RetryRun {
            run: stopped.id,
            notes: Some("use the helper".to_owned())
        }
    );

    // Cancel asks first.
    press(&mut world.app, KeyCode::Char('x'));
    let actions = press(&mut world.app, KeyCode::Char('y'));
    assert_eq!(command(&actions).1, Command::CancelRun { run: stopped.id });

    // What does not apply is refused here, with why.
    press(&mut world.app, KeyCode::Down);
    assert_eq!(world.app.runs_selected, Some(building.id));
    let actions = press(&mut world.app, KeyCode::Char('r'));
    assert!(actions.is_empty(), "{actions:?}");
    let note = world.app.note.clone().unwrap();
    assert!(
        note.error && note.text == "Not while the run on #7 is building.",
        "{note:?}"
    );

    // Taking over opens the person's agent as soon as it is there.
    let actions = press(&mut world.app, KeyCode::Char('t'));
    let (req_id, sent) = command(&actions);
    assert_eq!(sent, Command::TakeOverRun { run: building.id });
    let container = Workspace {
        id: WorkspaceId::new(),
        project: world.project.id,
        kind: WorkspaceKind::Container {
            run: building.id,
            branch: building.branch.clone(),
        },
        created_at: Timestamp::from_millis(1),
    };
    let mut agent = session(container.id, "Claude Code", 9);
    agent.harness = "claude".to_owned();
    reply(
        &mut world,
        req_id,
        Ok(Reply::RunTakenOver {
            workspace: container.id,
            session: agent.id,
        }),
    );
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::WorkspaceUpserted(container)));
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::SessionUpserted(agent.clone())));
    assert_eq!(world.app.mode, Mode::Terminal(agent.id));

    // Back in the runs list: ⏎ opens it again, and h hands the run back.
    world.app.mode = Mode::Browse;
    world.app.view = View::Runs;
    let mut taken = building.clone();
    taken.stage = RunStage::TakenOver;
    upsert(&mut world, &taken);
    world.app.runs_selected = Some(building.id);
    assert!(render(&world.app).contains("⏎ open agent  h hand back  x cancel"));
    press(&mut world.app, KeyCode::Enter);
    assert_eq!(world.app.mode, Mode::Terminal(agent.id));
    world.app.mode = Mode::Browse;
    press(&mut world.app, KeyCode::Char('h'));
    let actions = press(&mut world.app, KeyCode::Enter);
    assert_eq!(
        command(&actions).1,
        Command::HandBackRun { run: building.id }
    );
}

#[test]
fn a_ready_run_goes_back_only_with_what_to_change() {
    let mut world = world();
    let mut ready = run(world.project.id, 42, RunStage::ReadyForYou);
    ready.pr = Some(PullRequest {
        number: 17,
        url: "https://github.com/o/app/pull/17".to_owned(),
    });
    upsert(&mut world, &ready);
    to_runs(&mut world);
    assert!(render(&world.app).contains("PR       #17 https://github.com/o/app/pull/17"));
    press(&mut world.app, KeyCode::Char('c'));
    let actions = press(&mut world.app, KeyCode::Enter);
    assert!(actions.is_empty());
    assert!(matches!(
        world.app.mode,
        Mode::Dialog(Dialog::RunChanges { .. })
    ));
    assert!(
        world
            .app
            .note
            .clone()
            .unwrap()
            .text
            .starts_with("Say what to change")
    );
    typed(&mut world.app, "Handle the empty case.");
    let actions = press(&mut world.app, KeyCode::Enter);
    assert_eq!(
        command(&actions).1,
        Command::RequestChanges {
            run: ready.id,
            comments: "Handle the empty case.".to_owned()
        }
    );
    // A refusal from the daemon is shown as it says it.
    let (req_id, _) = command(&actions);
    reply(
        &mut world,
        req_id,
        Err(ErrorBody::new(
            ErrorCode::Conflict,
            "the run on #42 has ended",
        )),
    );
    assert_eq!(
        world.app.note.clone().unwrap().text,
        "the run on #42 has ended"
    );
}

#[test]
fn an_issue_is_handed_to_the_autopilot_from_the_issues_dialog() {
    let mut world = world();
    to_runs(&mut world);
    let actions = press(&mut world.app, KeyCode::Char('n'));
    let (req_id, listed) = command(&actions);
    assert_eq!(
        listed,
        Command::ListIssues {
            project: world.project.id
        }
    );
    let Mode::Dialog(Dialog::Issues { work, .. }) = &world.app.mode else {
        panic!("expected the issues dialog");
    };
    assert_eq!(*work, IssueWork::Autopilot);
    reply(
        &mut world,
        req_id,
        Ok(Reply::Issues {
            issues: vec![muster_core::Issue {
                number: 42,
                title: "Fix the login redirect".to_owned(),
                url: "https://github.com/o/app/issues/42".to_owned(),
                labels: Vec::new(),
                author: None,
            }],
        }),
    );
    let screen = render(&world.app);
    assert!(screen.contains("⏎ hand to the autopilot"), "{screen}");
    let actions = press(&mut world.app, KeyCode::Enter);
    let (req_id, started) = command(&actions);
    assert_eq!(
        started,
        Command::StartRun {
            project: world.project.id,
            number: 42
        }
    );
    let picked = run(world.project.id, 42, RunStage::Queued);
    reply(&mut world, req_id, Ok(Reply::RunStarted { id: picked.id }));
    world.app.view = super::View::Sessions;
    upsert(&mut world, &picked);
    assert_eq!(world.app.view, View::Runs);
    assert_eq!(world.app.runs_selected, Some(picked.id));

    // From the sessions view, ⇥ in the dialog reaches the autopilot too.
    world.app.view = View::Sessions;
    world.app.mode = Mode::Browse;
    press(&mut world.app, KeyCode::Char('i'));
    for expected in [
        IssueWork::Session(super::HarnessChoice::Shell),
        IssueWork::Autopilot,
        IssueWork::Session(super::HarnessChoice::Claude),
    ] {
        press(&mut world.app, KeyCode::Tab);
        let Mode::Dialog(Dialog::Issues { work, .. }) = &world.app.mode else {
            panic!("expected the issues dialog");
        };
        assert_eq!(*work, expected);
    }
}

#[test]
fn runs_waiting_on_the_person_join_needs_you_and_tell_them_while_away() {
    let mut world = world();
    world.app.on_focus(false);
    let mut going = run(world.project.id, 42, RunStage::Building);
    assert!(
        !upsert(&mut world, &going)
            .iter()
            .any(|action| matches!(action, Action::Notify(_)))
    );
    going.stage = RunStage::NeedsYou;
    going.reason = Some("it reached its time limit (90 min)".to_owned());
    let actions = upsert(&mut world, &going);
    assert!(
        actions.contains(&Action::Notify(
            "The run on #42 needs you: it reached its time limit (90 min)".to_owned()
        )),
        "{actions:?}"
    );
    // Told once, not on every change while it waits.
    going.cost_micros = Some(1);
    assert!(
        !upsert(&mut world, &going)
            .iter()
            .any(|action| matches!(action, Action::Notify(_)))
    );

    let mut ready = run(world.project.id, 7, RunStage::AgentReview);
    upsert(&mut world, &ready);
    ready.stage = RunStage::ReadyForYou;
    ready.pr = Some(PullRequest {
        number: 17,
        url: "https://github.com/o/app/pull/17".to_owned(),
    });
    let actions = upsert(&mut world, &ready);
    assert!(actions.contains(&Action::Notify(
        "The run on #7 is ready for you: pull request #17".to_owned()
    )));

    // In the queue: a run stopped for the person before one ready for them.
    press(&mut world.app, KeyCode::Tab);
    assert_eq!(world.app.view, View::NeedsYou);
    assert_eq!(
        world.app.needs_list,
        [Need::Run(going.id), Need::Run(ready.id)]
    );
    let screen = render(&world.app);
    assert!(screen.contains("2 need you"), "{screen}");
    assert!(screen.contains("▲ #42 needs you"), "{screen}");
    assert!(screen.contains("✓ #7 ready for you"), "{screen}");
    // Selected there, a run's details and keys are as in the runs view.
    assert!(
        screen.contains("Stage    needs you: it reached its time limit"),
        "{screen}"
    );
    assert!(screen.contains("r retry"), "{screen}");
}

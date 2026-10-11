use std::path::PathBuf;

use muster_core::{Project, ProjectId, PullRequest, Ticket};

use super::*;

const MINUTE: i64 = 60_000;

fn project(name: &str) -> Project {
    Project {
        id: ProjectId::new(),
        name: name.to_owned(),
        root: PathBuf::from(format!("/src/{name}")),
        created_at: Timestamp::from_millis(0),
    }
}

fn run(project: ProjectId, number: u64, stage: RunStage, created: i64) -> Run {
    Run {
        id: RunId::new(),
        project,
        ticket: Ticket {
            number,
            title: "Fix the login redirect".to_owned(),
            url: format!("https://github.com/o/app/issues/{number}"),
            body: String::new(),
            labels: Vec::new(),
            author: None,
        },
        branch: format!("muster/issue-{number}-fix-the-login-redirect"),
        stage,
        attempt: 1,
        max_attempts: 3,
        reason: None,
        pr: None,
        cost_micros: None,
        created_at: Timestamp::from_millis(created),
        ended_at: stage
            .is_over()
            .then_some(Timestamp::from_millis(created + MINUTE)),
    }
}

fn snapshot(projects: Vec<Project>, runs: Vec<Run>) -> Snapshot {
    Snapshot {
        projects,
        runs,
        ..Snapshot::default()
    }
}

#[test]
fn a_run_is_found_by_its_id_or_its_issue() {
    let app = project("app");
    let older = run(app.id, 42, RunStage::Cancelled, 0);
    let going = run(app.id, 42, RunStage::Building, MINUTE);
    let other = run(app.id, 7, RunStage::Merged, 0);
    let snapshot = snapshot(vec![app], vec![older.clone(), going.clone(), other.clone()]);

    assert_eq!(
        find_run(&snapshot, &older.id.to_string()).unwrap().id,
        older.id
    );
    // By issue: the run going on, whichever form the number takes.
    assert_eq!(find_run(&snapshot, "42").unwrap().id, going.id);
    assert_eq!(find_run(&snapshot, "#42").unwrap().id, going.id);
    // With none going, the latest.
    assert_eq!(find_run(&snapshot, "7").unwrap().id, other.id);
    assert_eq!(find_run(&snapshot, "8").unwrap_err(), "no run on issue #8");
    assert!(
        find_run(&snapshot, &RunId::new().to_string())
            .unwrap_err()
            .starts_with("no run ")
    );
    assert!(
        find_run(&snapshot, "fix")
            .unwrap_err()
            .contains("neither a run's id nor an issue's number")
    );
}

#[test]
fn the_same_issue_number_in_two_projects_needs_the_run_s_id() {
    let (app, web) = (project("app"), project("web"));
    let snapshot = snapshot(
        vec![app.clone(), web.clone()],
        vec![
            run(app.id, 42, RunStage::Building, 0),
            run(web.id, 42, RunStage::NeedsYou, 0),
        ],
    );
    assert!(
        find_run(&snapshot, "42")
            .unwrap_err()
            .contains("several projects")
    );
    let ended = self::snapshot(
        vec![app.clone(), web.clone()],
        vec![
            run(app.id, 42, RunStage::Merged, 0),
            run(web.id, 42, RunStage::Cancelled, MINUTE),
        ],
    );
    assert!(
        find_run(&ended, "42")
            .unwrap_err()
            .contains("several projects")
    );
}

#[test]
fn a_run_that_needs_you_says_why_how_it_got_there_and_what_you_can_do() {
    let app = project("app");
    let mut needs_you = run(app.id, 42, RunStage::NeedsYou, 0);
    needs_you.attempt = 3;
    needs_you.reason = Some("the checks failed after 3 attempt(s)\x1b[2J".to_owned());
    needs_you.cost_micros = Some(1_234_567);
    needs_you.pr = Some(PullRequest {
        number: 17,
        url: "https://github.com/o/app/pull/17".to_owned(),
    });
    let transitions = [
        (RunStage::Provisioning, 0, "there is room to run"),
        (RunStage::Building, 12_000, "its container is ready"),
        (
            RunStage::NeedsYou,
            65 * MINUTE,
            "the checks failed after 3 attempt(s)",
        ),
    ]
    .map(|(to, at, reason)| RunTransition {
        from: RunStage::Queued,
        to,
        reason: reason.to_owned(),
        at: Timestamp::from_millis(at),
    });
    let shown = describe(
        &snapshot(vec![app], vec![]),
        &needs_you,
        &transitions,
        Timestamp::from_millis(70 * MINUTE),
    );
    let expected = format!(
        "Issue #42 in app: Fix the login redirect
https://github.com/o/app/issues/42

stage         needs you: the checks failed after 3 attempt(s)[2J
branch        muster/issue-42-fix-the-login-redirect
attempt       3 of 3
pull request  #17 https://github.com/o/app/pull/17
cost          $1.23
time          picked up 1 h 10 min ago
run           {}

Timeline
  +0 s         starting   there is room to run
  +12 s        building   its container is ready
  +1 h 05 min  needs you  the checks failed after 3 attempt(s)

`muster run retry 42 [--notes …]` sends it back to work with a fresh budget;
`muster run take-over 42` lets you drive its agent; `muster run cancel 42` ends it.
",
        needs_you.id
    );
    assert_eq!(shown, expected);
}

#[test]
fn a_ready_run_points_at_its_pull_request_and_an_ended_one_at_nothing() {
    let app = project("app");
    let mut ready = run(app.id, 42, RunStage::ReadyForYou, 0);
    ready.pr = Some(PullRequest {
        number: 17,
        url: "https://github.com/o/app/pull/17".to_owned(),
    });
    let snapshot = snapshot(vec![app.clone()], vec![]);
    let shown = describe(&snapshot, &ready, &[], Timestamp::from_millis(MINUTE));
    assert!(
        shown.contains("Review pull request #17 on GitHub."),
        "{shown}"
    );
    assert!(shown.contains("`muster run changes 42 \"…\"`"), "{shown}");
    assert!(!shown.contains("Timeline"));

    let merged = run(app.id, 42, RunStage::Merged, 0);
    let shown = describe(&snapshot, &merged, &[], Timestamp::from_millis(3 * MINUTE));
    assert!(
        shown.contains("time          ended 2 min ago, after 1 min"),
        "{shown}"
    );
    assert!(!shown.contains("muster run"), "{shown}");
}

#[test]
fn lists_put_what_waits_on_you_first_and_long_titles_are_cut() {
    let mut stages = [
        RunStage::Cancelled,
        RunStage::Queued,
        RunStage::Building,
        RunStage::ReadyForYou,
        RunStage::NeedsYou,
        RunStage::Merged,
    ];
    stages.sort_by_key(|stage| stage.list_order());
    assert_eq!(
        stages,
        [
            RunStage::NeedsYou,
            RunStage::ReadyForYou,
            RunStage::Building,
            RunStage::Queued,
            RunStage::Merged,
            RunStage::Cancelled,
        ]
    );
    assert_eq!(cut("short", 10), "short");
    assert_eq!(cut("ünïcode title", 6), "ünïco…");
    assert_eq!(cost(Some(10_000)), Some("$0.01".to_owned()));
    assert_eq!(cost(None), None);
}

use std::time::{Duration, Instant};

use muster_core::RunStage;

use super::{AgentOutcome, Feedback, Limits, RunEffect, RunEvent, RunMachine, Verdict};

/// A run with a fake clock that only moves when told to.
struct Run {
    machine: RunMachine,
    start: Instant,
    now: Instant,
}

impl Run {
    fn new() -> Self {
        Self::with(Limits::default())
    }

    fn with(limits: Limits) -> Self {
        let start = Instant::now();
        Self {
            machine: RunMachine::new(limits),
            start,
            now: start,
        }
    }

    fn at(&mut self, minutes: u64) -> &mut Self {
        self.now = self.start + Duration::from_secs(minutes * 60);
        self
    }

    fn send(&mut self, event: RunEvent) -> Vec<RunEffect> {
        self.machine.step(event, self.now)
    }

    fn generation(&self) -> u64 {
        self.machine.generation()
    }

    fn stage(&self) -> RunStage {
        self.machine.stage()
    }

    // Reports on the work asked for now.

    fn container_ready(&mut self) -> Vec<RunEffect> {
        let generation = self.generation();
        self.send(RunEvent::ContainerReady { generation })
    }

    fn agent_done(&mut self) -> Vec<RunEffect> {
        let generation = self.generation();
        self.send(RunEvent::AgentFinished {
            generation,
            outcome: AgentOutcome::Done,
        })
    }

    fn checks(&mut self, passed: bool) -> Vec<RunEffect> {
        let generation = self.generation();
        self.send(RunEvent::ChecksFinished {
            generation,
            passed,
            report: if passed {
                String::new()
            } else {
                "test failed".to_owned()
            },
        })
    }

    fn pr_opened(&mut self) -> Vec<RunEffect> {
        let generation = self.generation();
        self.send(RunEvent::PrOpened {
            generation,
            number: 17,
        })
    }

    fn review(&mut self, verdict: Verdict) -> Vec<RunEffect> {
        let generation = self.generation();
        self.send(RunEvent::ReviewSubmitted {
            generation,
            verdict,
        })
    }

    fn infra(&mut self, container_lost: bool) -> Vec<RunEffect> {
        let generation = self.generation();
        self.send(RunEvent::InfraFailed {
            generation,
            reason: "docker went away".to_owned(),
            container_lost,
        })
    }

    /// A run that has got to `stage` the usual way.
    fn at_stage(stage: RunStage) -> Self {
        let mut run = Self::new();
        let path = [
            RunStage::Provisioning,
            RunStage::Building,
            RunStage::Checking,
            RunStage::OpeningPr,
            RunStage::AgentReview,
            RunStage::ReadyForYou,
        ];
        for next in path {
            if run.stage() == stage {
                break;
            }
            match next {
                RunStage::Provisioning => run.send(RunEvent::SlotGranted),
                RunStage::Building => run.container_ready(),
                RunStage::Checking => run.agent_done(),
                RunStage::OpeningPr => run.checks(true),
                RunStage::AgentReview => run.pr_opened(),
                _ => run.review(Verdict::Approve),
            };
        }
        assert_eq!(run.stage(), stage);
        run
    }
}

fn transition(effects: &[RunEffect]) -> (RunStage, RunStage, String) {
    match effects.first() {
        Some(RunEffect::Transition { from, to, reason }) => (*from, *to, reason.clone()),
        other => panic!("the first effect is not a transition: {other:?} in {effects:?}"),
    }
}

fn work(effects: &[RunEffect]) -> &[RunEffect] {
    &effects[1..]
}

fn builder(effects: &[RunEffect]) -> (u32, Option<Feedback>) {
    effects
        .iter()
        .find_map(|effect| match effect {
            RunEffect::StartBuilder {
                attempt, feedback, ..
            } => Some((*attempt, feedback.clone())),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no builder started in {effects:?}"))
}

// The way through

#[test]
fn a_run_goes_from_ticket_to_merged_with_a_recorded_reason_at_each_step() {
    let mut run = Run::new();
    assert_eq!(run.stage(), RunStage::Queued);

    let effects = run.send(RunEvent::SlotGranted);
    assert_eq!(
        transition(&effects),
        (
            RunStage::Queued,
            RunStage::Provisioning,
            "there is room to run".to_owned()
        )
    );
    assert_eq!(
        work(&effects),
        [RunEffect::StartContainer {
            generation: run.generation()
        }]
    );

    let effects = run.container_ready();
    assert_eq!(transition(&effects).1, RunStage::Building);
    assert_eq!(builder(&effects), (1, None));

    let effects = run.agent_done();
    assert_eq!(
        work(&effects),
        [RunEffect::RunChecks {
            generation: run.generation()
        }]
    );

    let effects = run.checks(true);
    assert_eq!(transition(&effects).2, "the checks passed");
    assert_eq!(
        work(&effects),
        [RunEffect::PublishBranch {
            generation: run.generation()
        }]
    );

    let effects = run.pr_opened();
    assert_eq!(transition(&effects).2, "pull request #17 is open");
    assert_eq!(run.machine.pr(), Some(17));
    assert_eq!(
        work(&effects),
        [RunEffect::StartReviewer {
            generation: run.generation()
        }]
    );

    let effects = run.review(Verdict::Approve);
    assert_eq!(transition(&effects).1, RunStage::ReadyForYou);
    assert_eq!(work(&effects), [RunEffect::MarkReady]);

    let effects = run.send(RunEvent::PrMerged);
    assert_eq!(transition(&effects).1, RunStage::Merged);
    assert_eq!(work(&effects), [RunEffect::RemoveContainer]);
    assert!(run.send(RunEvent::Cancel).is_empty(), "over is over");
}

// Attempts

#[test]
fn failed_checks_go_back_to_the_agent_with_the_report() {
    let mut run = Run::at_stage(RunStage::Checking);
    let effects = run.checks(false);
    assert_eq!(transition(&effects).2, "the checks failed; attempt 2");
    assert_eq!(
        builder(&effects),
        (
            2,
            Some(Feedback::ChecksFailed {
                report: "test failed".to_owned()
            })
        )
    );
    run.agent_done();
    run.checks(true);
    assert_eq!(run.stage(), RunStage::OpeningPr);
}

#[test]
fn spent_attempts_stop_for_the_person_and_a_retry_starts_a_fresh_budget() {
    let mut run = Run::at_stage(RunStage::Checking);
    run.checks(false); // attempt 2
    run.agent_done();
    run.checks(false); // attempt 3
    run.agent_done();
    let effects = run.checks(false);
    assert_eq!(
        transition(&effects),
        (
            RunStage::Checking,
            RunStage::NeedsYou,
            "the checks failed after 3 attempt(s)".to_owned()
        )
    );
    assert_eq!(work(&effects), []);
    assert_eq!(
        run.machine.reason(),
        Some("the checks failed after 3 attempt(s)")
    );

    // Without notes, the agent gets the last report again.
    let effects = run.send(RunEvent::Retry { notes: None });
    assert_eq!(transition(&effects).1, RunStage::Building);
    assert_eq!(
        builder(&effects),
        (
            4,
            Some(Feedback::ChecksFailed {
                report: "test failed".to_owned()
            })
        )
    );
    assert_eq!(run.machine.reason(), None);
    // A fresh budget: three more attempts before stopping again.
    for _ in 0..2 {
        run.agent_done();
        run.checks(false);
        assert_eq!(run.stage(), RunStage::Building);
    }
    run.agent_done();
    run.checks(false);
    assert_eq!(run.stage(), RunStage::NeedsYou);

    let effects = run.send(RunEvent::Retry {
        notes: Some("the flaky test is unrelated; skip it".to_owned()),
    });
    assert_eq!(
        builder(&effects).1,
        Some(Feedback::PersonNotes {
            notes: "the flaky test is unrelated; skip it".to_owned()
        })
    );
}

#[test]
fn a_reviewer_and_a_person_asking_for_changes_each_spend_an_attempt() {
    let mut run = Run::at_stage(RunStage::AgentReview);
    let effects = run.review(Verdict::RequestChanges {
        comments: "handle the empty case".to_owned(),
    });
    assert_eq!(
        transition(&effects).2,
        "the reviewer asked for changes; attempt 2"
    );
    assert_eq!(
        builder(&effects).1,
        Some(Feedback::ReviewerRequestedChanges {
            comments: "handle the empty case".to_owned()
        })
    );
    run.agent_done();
    run.checks(true);
    // The pull request exists; publishing pushes to it.
    run.pr_opened();
    run.review(Verdict::Approve);
    assert_eq!(run.stage(), RunStage::ReadyForYou);

    let effects = run.send(RunEvent::PersonRequestedChanges {
        comments: "rename it".to_owned(),
    });
    assert_eq!(transition(&effects).1, RunStage::Building);
    assert_eq!(effects[1], RunEffect::MarkDraft);
    assert_eq!(
        builder(&effects),
        (
            3,
            Some(Feedback::PersonRequestedChanges {
                comments: "rename it".to_owned()
            })
        )
    );

    // The budget is spent now; one more request stops for the person.
    run.agent_done();
    run.checks(true);
    run.pr_opened();
    run.review(Verdict::Approve);
    run.send(RunEvent::PersonRequestedChanges {
        comments: "and this".to_owned(),
    });
    assert_eq!(run.stage(), RunStage::NeedsYou);
    assert_eq!(
        run.machine.reason(),
        Some("you asked for changes after 3 attempt(s)")
    );
}

#[test]
fn an_attempt_budget_of_zero_still_allows_one() {
    let mut run = Run::with(Limits {
        attempts: 0,
        ..Limits::default()
    });
    run.send(RunEvent::SlotGranted);
    run.container_ready();
    run.agent_done();
    run.checks(false);
    assert_eq!(run.stage(), RunStage::NeedsYou);
}

// Failures

#[test]
fn an_agent_that_stops_on_an_error_needs_the_person() {
    let mut run = Run::at_stage(RunStage::Building);
    let generation = run.generation();
    let effects = run.send(RunEvent::AgentFinished {
        generation,
        outcome: AgentOutcome::Failed {
            reason: "API overloaded".to_owned(),
        },
    });
    assert_eq!(transition(&effects).2, "the agent stopped: API overloaded");
    let effects = run.send(RunEvent::Retry { notes: None });
    assert_eq!(builder(&effects).0, 2);
}

#[test]
fn a_reviewer_without_a_usable_verdict_needs_the_person() {
    let mut run = Run::at_stage(RunStage::AgentReview);
    run.agent_done();
    assert_eq!(run.stage(), RunStage::NeedsYou);
    // A retry asks for the review again.
    let effects = run.send(RunEvent::Retry { notes: None });
    assert!(matches!(work(&effects), [RunEffect::StartReviewer { .. }]));
}

#[test]
fn an_infrastructure_failure_stops_the_work_and_a_retry_resumes_there() {
    let mut run = Run::at_stage(RunStage::OpeningPr);
    let effects = run.infra(false);
    assert_eq!(
        transition(&effects),
        (
            RunStage::OpeningPr,
            RunStage::NeedsYou,
            "docker went away".to_owned()
        )
    );
    assert_eq!(work(&effects), [RunEffect::StopWork]);
    let effects = run.send(RunEvent::Retry { notes: None });
    assert!(matches!(work(&effects), [RunEffect::PublishBranch { .. }]));
}

#[test]
fn a_lost_container_starts_over_in_a_new_one_with_the_persons_notes() {
    let mut run = Run::at_stage(RunStage::Building);
    run.infra(true);
    let effects = run.send(RunEvent::Retry {
        notes: Some("start from main again".to_owned()),
    });
    assert_eq!(transition(&effects).1, RunStage::Provisioning);
    let effects = run.container_ready();
    assert_eq!(
        builder(&effects),
        (
            2,
            Some(Feedback::PersonNotes {
                notes: "start from main again".to_owned()
            })
        )
    );
    // And nothing to remove when cancelled before a container is made.
    let mut early = Run::at_stage(RunStage::Provisioning);
    early.infra(false);
    let effects = early.send(RunEvent::Cancel);
    assert_eq!(work(&effects), []);
}

// Limits

#[test]
fn time_counts_only_while_the_run_works_on_its_own() {
    let mut run = Run::with(Limits {
        run_timeout: Duration::from_secs(60 * 60),
        ..Limits::default()
    });
    assert_eq!(run.machine.next_deadline(), None, "queued: not working");
    run.at(10).send(RunEvent::SlotGranted);
    assert_eq!(
        run.machine.next_deadline(),
        Some(run.start + Duration::from_secs(70 * 60))
    );
    run.at(20).container_ready();
    run.agent_done();
    run.checks(true);
    run.pr_opened();
    // 30 minutes spent, then three hours waiting on the person.
    run.at(40).review(Verdict::Approve);
    assert_eq!(run.machine.next_deadline(), None);
    run.at(220).send(RunEvent::PersonRequestedChanges {
        comments: "more".to_owned(),
    });
    assert_eq!(
        run.machine.next_deadline(),
        Some(run.start + Duration::from_secs(250 * 60)),
        "the 30 minutes left of the hour"
    );
    assert!(run.at(249).send(RunEvent::Tick).is_empty());
    let effects = run.at(250).send(RunEvent::Tick);
    assert_eq!(transition(&effects).2, "it reached its time limit (60 min)");
    assert_eq!(work(&effects), [RunEffect::StopWork]);
    assert_eq!(run.machine.next_deadline(), None);

    // A retry starts a fresh hour.
    run.at(300).send(RunEvent::Retry { notes: None });
    assert_eq!(
        run.machine.next_deadline(),
        Some(run.start + Duration::from_secs(360 * 60))
    );
}

#[test]
fn the_cost_limit_stops_work_and_spending_resumes_with_a_fresh_budget() {
    let mut run = Run::with(Limits {
        max_cost_micros: Some(5_000_000),
        ..Limits::default()
    });
    run.send(RunEvent::SlotGranted);
    run.container_ready();
    assert!(
        run.send(RunEvent::CostReported { micros: 4_999_999 })
            .is_empty()
    );
    let effects = run.send(RunEvent::CostReported { micros: 5_000_000 });
    assert_eq!(transition(&effects).2, "it reached its cost limit");
    assert_eq!(work(&effects), [RunEffect::StopWork]);
    // Reports while it waits change nothing.
    assert!(
        run.send(RunEvent::CostReported { micros: 9_000_000 })
            .is_empty()
    );
    run.send(RunEvent::Retry { notes: None });
    assert!(
        run.send(RunEvent::CostReported { micros: 13_999_999 })
            .is_empty()
    );
    run.send(RunEvent::CostReported { micros: 14_000_000 });
    assert_eq!(run.stage(), RunStage::NeedsYou);
}

// The person's commands

#[test]
fn cancelling_stops_the_work_and_removes_the_container() {
    let effects = Run::new().send(RunEvent::Cancel);
    assert_eq!(transition(&effects).1, RunStage::Cancelled);
    assert_eq!(work(&effects), []);

    let mut run = Run::at_stage(RunStage::Checking);
    let effects = run.send(RunEvent::Cancel);
    assert_eq!(
        work(&effects),
        [RunEffect::StopWork, RunEffect::RemoveContainer]
    );

    let mut run = Run::at_stage(RunStage::ReadyForYou);
    let effects = run.send(RunEvent::Cancel);
    assert_eq!(work(&effects), [RunEffect::RemoveContainer]);
}

#[test]
fn taking_over_stops_the_agent_and_handing_back_checks_the_persons_work() {
    let mut run = Run::at_stage(RunStage::Building);
    let effects = run.send(RunEvent::TakeOver);
    assert_eq!(transition(&effects).1, RunStage::TakenOver);
    assert_eq!(
        work(&effects),
        [RunEffect::StopWork, RunEffect::StartInteractive]
    );
    assert_eq!(run.machine.next_deadline(), None, "the person's time");

    let effects = run.send(RunEvent::HandBack);
    assert_eq!(transition(&effects).2, "you handed it back");
    assert!(matches!(
        work(&effects),
        [RunEffect::StopWork, RunEffect::RunChecks { .. }]
    ));
}

#[test]
fn a_ready_pull_request_goes_back_to_draft_when_taken_over() {
    let mut run = Run::at_stage(RunStage::ReadyForYou);
    let effects = run.send(RunEvent::TakeOver);
    assert_eq!(
        work(&effects),
        [RunEffect::MarkDraft, RunEffect::StartInteractive]
    );
}

#[test]
fn there_is_nothing_to_take_over_before_the_container() {
    for stage in [RunStage::Queued, RunStage::Provisioning] {
        let mut run = Run::at_stage(stage);
        assert!(run.send(RunEvent::TakeOver).is_empty(), "{stage:?}");
    }
}

#[test]
fn commands_that_do_not_apply_change_nothing() {
    let mut run = Run::at_stage(RunStage::Building);
    assert!(run.send(RunEvent::HandBack).is_empty());
    assert!(run.send(RunEvent::Retry { notes: None }).is_empty());
    assert!(run.send(RunEvent::SlotGranted).is_empty());
    assert!(
        run.send(RunEvent::PrMerged).is_empty(),
        "no pull request yet"
    );
    assert!(
        run.send(RunEvent::PersonRequestedChanges {
            comments: String::new()
        })
        .is_empty()
    );
}

// Odd orders

#[test]
fn reports_on_overtaken_work_are_ignored() {
    // Checks finishing after the person took over.
    let mut run = Run::at_stage(RunStage::Checking);
    let stale = run.generation();
    run.send(RunEvent::TakeOver);
    assert!(
        run.send(RunEvent::ChecksFinished {
            generation: stale,
            passed: true,
            report: String::new(),
        })
        .is_empty()
    );
    assert_eq!(run.stage(), RunStage::TakenOver);

    // The first attempt finishing while the second builds.
    let mut run = Run::at_stage(RunStage::Checking);
    let first = run.generation() - 1;
    run.checks(false);
    assert!(
        run.send(RunEvent::AgentFinished {
            generation: first,
            outcome: AgentOutcome::Done,
        })
        .is_empty()
    );
    assert_eq!(run.stage(), RunStage::Building);

    // A review arriving after a cancel.
    let mut run = Run::at_stage(RunStage::AgentReview);
    let generation = run.generation();
    run.send(RunEvent::Cancel);
    assert!(
        run.send(RunEvent::ReviewSubmitted {
            generation,
            verdict: Verdict::Approve,
        })
        .is_empty()
    );
    assert_eq!(run.stage(), RunStage::Cancelled);
}

#[test]
fn a_merge_while_checks_run_again_ends_the_run() {
    let mut run = Run::at_stage(RunStage::ReadyForYou);
    run.send(RunEvent::PersonRequestedChanges {
        comments: "tweak".to_owned(),
    });
    run.agent_done();
    assert_eq!(run.stage(), RunStage::Checking);
    let effects = run.send(RunEvent::PrMerged);
    assert_eq!(transition(&effects).1, RunStage::Merged);
    assert_eq!(
        work(&effects),
        [RunEffect::StopWork, RunEffect::RemoveContainer]
    );
}

#[test]
fn a_limit_hit_during_review_stops_the_reviewer() {
    let mut run = Run::with(Limits {
        run_timeout: Duration::from_secs(60),
        ..Limits::default()
    });
    run.send(RunEvent::SlotGranted);
    run.container_ready();
    run.agent_done();
    run.checks(true);
    run.pr_opened();
    assert_eq!(run.stage(), RunStage::AgentReview);
    let effects = run.at(1).send(RunEvent::Tick);
    assert_eq!(transition(&effects).1, RunStage::NeedsYou);
    assert_eq!(work(&effects), [RunEffect::StopWork]);
}

#[test]
fn a_pull_request_closed_unmerged_cancels_the_run() {
    let mut run = Run::at_stage(RunStage::ReadyForYou);
    let effects = run.send(RunEvent::PrClosed);
    assert_eq!(
        transition(&effects),
        (
            RunStage::ReadyForYou,
            RunStage::Cancelled,
            "the pull request was closed without merging".to_owned()
        )
    );
    assert_eq!(work(&effects), [RunEffect::RemoveContainer]);
}

#[test]
fn every_transition_comes_first_and_bumps_the_generation() {
    let mut run = Run::at_stage(RunStage::Building);
    let before = run.generation();
    let effects = run.send(RunEvent::TakeOver);
    assert!(matches!(effects[0], RunEffect::Transition { .. }));
    assert_eq!(run.generation(), before + 1);
    let effects = run.send(RunEvent::Cancel);
    assert!(matches!(effects[0], RunEffect::Transition { .. }));
}

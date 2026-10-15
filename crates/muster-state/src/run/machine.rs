//! The run machine.

use std::time::{Duration, Instant};

use muster_core::RunStage;
use serde::{Deserialize, Serialize};

use super::signal::{AgentOutcome, Feedback, RunEffect, RunEvent, Verdict};

/// What a run may spend before it stops for the person (`PRD-12`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Limits {
    /// Building attempts per budget; a failed check, a reviewer's and a
    /// person's request for changes each start another (`PRD-02`).
    pub attempts: u32,
    /// Time spent working on its own per budget; time waiting on the
    /// person does not count.
    pub run_timeout: Duration,
    /// Cost per budget, in millionths of a dollar, where the harness
    /// reports it.
    pub max_cost_micros: Option<u64>,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            attempts: 3,
            run_timeout: Duration::from_mins(90),
            max_cost_micros: None,
        }
    }
}

/// Takes one ticket through the stages of `PRODUCT.md` §5.
///
/// Pure: no IO, and time comes in with every call (`ARC-01`). A person's
/// retry or hand-back starts a fresh budget of attempts, time and cost.
///
/// It is stored with its run, so a daemon restart resumes the run
/// (`PRD-09`): [`save`](Self::save) folds the current stretch of work into
/// the time spent, and [`restore`](Self::restore) starts a new stretch.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RunMachine {
    limits: Limits,
    stage: RunStage,
    /// Bumped on every transition; work is asked for under it and reports
    /// back with it, so reports on overtaken work are recognised.
    generation: u64,
    /// Building attempts started in all.
    attempt: u32,
    /// Attempts left in the current budget.
    attempts_left: u32,
    pr: Option<u64>,
    has_container: bool,
    /// Where a run that needs the person goes back to on a retry.
    resume: Option<RunStage>,
    /// What the last attempt was told, for a retry from `NeedsYou`.
    last_feedback: Option<Feedback>,
    /// What the first attempt in a new container is to be told.
    pending_feedback: Option<Feedback>,
    /// Why it needs the person, while it does.
    reason: Option<String>,
    /// When the current stretch of work on its own began.
    #[serde(skip)]
    active_since: Option<Instant>,
    /// Work on its own in the current budget before that stretch.
    active_spent: Duration,
    cost_micros: u64,
    /// The cost when the current budget began.
    cost_at_budget: u64,
    /// The run took over a person's branch (`PRD-08`): its first container
    /// goes to the checks with their work rather than to the building agent.
    #[serde(default)]
    from_branch: bool,
}

impl RunMachine {
    /// A run just picked up: queued for room to run.
    pub fn new(limits: Limits) -> Self {
        Self {
            limits: Limits {
                attempts: limits.attempts.max(1),
                ..limits
            },
            stage: RunStage::Queued,
            generation: 0,
            attempt: 0,
            attempts_left: limits.attempts.max(1),
            pr: None,
            has_container: false,
            resume: None,
            last_feedback: None,
            pending_feedback: None,
            reason: None,
            active_since: None,
            active_spent: Duration::ZERO,
            cost_micros: 0,
            cost_at_budget: 0,
            from_branch: false,
        }
    }

    /// A run handed the work on a person's branch: it starts at the checks,
    /// and goes to the building agent only if they fail (`PRODUCT.md` §5).
    pub fn handed_off(limits: Limits) -> Self {
        Self {
            from_branch: true,
            ..Self::new(limits)
        }
    }

    /// Whether the run took over a person's branch.
    pub fn from_branch(&self) -> bool {
        self.from_branch
    }

    pub fn stage(&self) -> RunStage {
        self.stage
    }

    /// The machine as it is to be stored, at `now`.
    pub fn save(&self, now: Instant) -> Self {
        let mut saved = self.clone();
        if let Some(since) = saved.active_since.take() {
            saved.active_spent += now.saturating_duration_since(since);
        }
        saved
    }

    /// A stored machine, carrying on at `now`: work on its own resumes its
    /// time budget from here.
    pub fn restore(mut saved: Self, now: Instant) -> Self {
        saved.active_since = saved.stage.is_active().then_some(now);
        saved
    }

    /// The work the current stage is doing, asked for again under the
    /// current generation, for a daemon that restarted while it was under
    /// way. Carrying it out must find the work already started for this
    /// generation rather than start it twice (`ARC-10`).
    pub fn resume_work(&self) -> Vec<RunEffect> {
        let generation = self.generation;
        match self.stage {
            RunStage::Provisioning => vec![RunEffect::StartContainer { generation }],
            RunStage::Building => vec![RunEffect::StartBuilder {
                generation,
                attempt: self.attempt,
                feedback: self.last_feedback.clone(),
            }],
            RunStage::Checking => vec![RunEffect::RunChecks { generation }],
            RunStage::OpeningPr => vec![RunEffect::PublishBranch { generation }],
            RunStage::AgentReview => vec![RunEffect::StartReviewer { generation }],
            RunStage::Queued
            | RunStage::ReadyForYou
            | RunStage::TakenOver
            | RunStage::NeedsYou
            | RunStage::Merged
            | RunStage::Cancelled => Vec::new(),
        }
    }

    /// The limits of a budget.
    pub fn limits(&self) -> Limits {
        self.limits
    }

    /// What the run has cost, where the harness reported it.
    pub fn cost_micros(&self) -> u64 {
        self.cost_micros
    }

    /// The generation work is asked for under now.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Building attempts started in all.
    pub fn attempt(&self) -> u32 {
        self.attempt
    }

    /// The pull request's number, once opened.
    pub fn pr(&self) -> Option<u64> {
        self.pr
    }

    /// Why the run needs the person, while it does.
    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }

    /// When the daemon should next send [`RunEvent::Tick`]: the end of the
    /// time budget, while the run works on its own.
    pub fn next_deadline(&self) -> Option<Instant> {
        let since = self.active_since?;
        Some(since + self.limits.run_timeout.saturating_sub(self.active_spent))
    }

    /// Applies one event and returns what the daemon must do about it.
    pub fn step(&mut self, event: RunEvent, now: Instant) -> Vec<RunEffect> {
        if self.stage.is_over() {
            return Vec::new();
        }
        if let Some(generation) = generation_of(&event)
            && generation != self.generation
        {
            // A report on work overtaken by a transition since.
            return Vec::new();
        }
        let mut effects = Vec::new();
        match event {
            RunEvent::Cancel => self.cancel(now, &mut effects),
            RunEvent::PrMerged if self.pr.is_some() => {
                self.stop_work(&mut effects);
                self.end(
                    RunStage::Merged,
                    "the pull request was merged",
                    now,
                    &mut effects,
                );
            }
            RunEvent::PrClosed if self.pr.is_some() => {
                self.stop_work(&mut effects);
                let reason = "the pull request was closed without merging";
                self.end(RunStage::Cancelled, reason, now, &mut effects);
            }
            RunEvent::InfraFailed {
                reason,
                container_lost,
                ..
            } if self.stage.is_active() => {
                effects.push(STOP_WORK);
                if container_lost {
                    // Its work went with it; a retry starts from a new one.
                    self.has_container = false;
                }
                self.needs_you(reason, now, &mut effects);
            }
            RunEvent::CostReported { micros } => {
                self.cost_micros = self.cost_micros.max(micros);
                let over = self
                    .limits
                    .max_cost_micros
                    .is_some_and(|max| self.cost_micros.saturating_sub(self.cost_at_budget) >= max);
                if over && self.stage.is_active() {
                    effects.push(STOP_WORK);
                    self.needs_you("it reached its cost limit".to_owned(), now, &mut effects);
                }
            }
            RunEvent::Tick => {
                if self.next_deadline().is_some_and(|deadline| now >= deadline) {
                    effects.push(STOP_WORK);
                    let limit = self.limits.run_timeout.as_secs();
                    let shown = if limit < 60 {
                        format!("{limit} s")
                    } else {
                        format!("{} min", limit.div_ceil(60))
                    };
                    let reason = format!("it reached its time limit ({shown})");
                    self.needs_you(reason, now, &mut effects);
                }
            }
            event => self.on_stage_event(event, now, &mut effects),
        }
        // Work is stopped below the generation the step moved to, so the
        // work it asks for next, under that generation, is not.
        for effect in &mut effects {
            if let RunEffect::StopWork { before } = effect {
                *before = self.generation;
            }
        }
        effects
    }

    fn on_stage_event(&mut self, event: RunEvent, now: Instant, effects: &mut Vec<RunEffect>) {
        match (self.stage, event) {
            (RunStage::Queued, RunEvent::SlotGranted) => {
                self.enter(
                    RunStage::Provisioning,
                    "there is room to run",
                    None,
                    now,
                    effects,
                );
            }
            (RunStage::Provisioning, RunEvent::ContainerReady { .. }) => {
                self.has_container = true;
                let feedback = self.pending_feedback.take();
                if self.from_branch && self.attempt == 0 && feedback.is_none() {
                    // The person's work, as they handed it over, goes to
                    // the checks first.
                    let reason = "its container is ready with your work";
                    self.enter(RunStage::Checking, reason, None, now, effects);
                } else {
                    self.enter(
                        RunStage::Building,
                        "its container is ready",
                        feedback,
                        now,
                        effects,
                    );
                }
            }
            (RunStage::Building, RunEvent::AgentFinished { outcome, .. }) => match outcome {
                AgentOutcome::Done => {
                    self.enter(RunStage::Checking, "the agent is done", None, now, effects);
                }
                AgentOutcome::Failed { reason } => {
                    self.needs_you(format!("the agent stopped: {reason}"), now, effects);
                }
            },
            (RunStage::Checking, RunEvent::ChecksFinished { passed, report, .. }) => {
                if passed {
                    self.enter(RunStage::OpeningPr, "the checks passed", None, now, effects);
                } else {
                    let feedback = Feedback::ChecksFailed { report };
                    self.another_attempt("the checks failed", feedback, now, effects);
                }
            }
            (RunStage::OpeningPr, RunEvent::PrOpened { number, .. }) => {
                self.pr = Some(number);
                let reason = format!("pull request #{number} is open");
                self.enter(RunStage::AgentReview, &reason, None, now, effects);
            }
            (RunStage::AgentReview, RunEvent::ReviewSubmitted { verdict, .. }) => match verdict {
                Verdict::Approve => {
                    self.enter(
                        RunStage::ReadyForYou,
                        "the reviewer approved",
                        None,
                        now,
                        effects,
                    );
                }
                Verdict::RequestChanges { comments } => {
                    let feedback = Feedback::ReviewerRequestedChanges { comments };
                    self.another_attempt("the reviewer asked for changes", feedback, now, effects);
                }
            },
            (RunStage::AgentReview, RunEvent::AgentFinished { .. }) => {
                // The reviewer ended without a verdict the daemon could use.
                self.needs_you(
                    "the reviewer gave no usable verdict".to_owned(),
                    now,
                    effects,
                );
            }
            (RunStage::ReadyForYou, RunEvent::PersonRequestedChanges { comments }) => {
                effects.push(RunEffect::MarkDraft);
                let feedback = Feedback::PersonRequestedChanges { comments };
                self.another_attempt("you asked for changes", feedback, now, effects);
            }
            (RunStage::NeedsYou, RunEvent::Retry { notes }) => self.retry(notes, now, effects),
            (
                RunStage::Building
                | RunStage::Checking
                | RunStage::AgentReview
                | RunStage::ReadyForYou
                | RunStage::NeedsYou,
                RunEvent::TakeOver,
            ) if self.has_container => {
                if self.stage.is_active() {
                    effects.push(STOP_WORK);
                }
                if self.stage == RunStage::ReadyForYou {
                    effects.push(RunEffect::MarkDraft);
                }
                self.enter(RunStage::TakenOver, "you took over", None, now, effects);
            }
            (RunStage::TakenOver, RunEvent::HandBack) => {
                // The interactive agent stops before its work is checked.
                self.stop_work(effects);
                self.fresh_budget();
                self.enter(RunStage::Checking, "you handed it back", None, now, effects);
            }
            // Anything else does not apply in this stage: an event that came
            // too late or too early, or a command that makes no sense now.
            _ => {}
        }
    }

    /// Moves to `stage` for `reason` and asks for the work that stage does.
    fn enter(
        &mut self,
        stage: RunStage,
        reason: &str,
        feedback: Option<Feedback>,
        now: Instant,
        effects: &mut Vec<RunEffect>,
    ) {
        self.transition(stage, reason.to_owned(), now, effects);
        let generation = self.generation;
        match stage {
            RunStage::Provisioning => effects.push(RunEffect::StartContainer { generation }),
            RunStage::Building => {
                self.attempt += 1;
                self.attempts_left = self.attempts_left.saturating_sub(1);
                self.last_feedback.clone_from(&feedback);
                effects.push(RunEffect::StartBuilder {
                    generation,
                    attempt: self.attempt,
                    feedback,
                });
            }
            RunStage::Checking => effects.push(RunEffect::RunChecks { generation }),
            RunStage::OpeningPr => effects.push(RunEffect::PublishBranch { generation }),
            RunStage::AgentReview => effects.push(RunEffect::StartReviewer { generation }),
            RunStage::ReadyForYou => effects.push(RunEffect::MarkReady),
            RunStage::TakenOver => effects.push(RunEffect::StartInteractive),
            RunStage::Queued | RunStage::NeedsYou | RunStage::Merged | RunStage::Cancelled => {}
        }
    }

    /// Sends the work back to the building agent with `feedback`, or to
    /// the person once the budget's attempts are spent.
    fn another_attempt(
        &mut self,
        why: &str,
        feedback: Feedback,
        now: Instant,
        effects: &mut Vec<RunEffect>,
    ) {
        if self.attempts_left == 0 {
            self.last_feedback = Some(feedback);
            let tried = self.limits.attempts;
            let reason = format!("{why} after {tried} attempt(s)");
            self.resume = Some(RunStage::Building);
            self.transition(RunStage::NeedsYou, reason.clone(), now, effects);
            self.reason = Some(reason);
            return;
        }
        let next = self.attempt + 1;
        let reason = format!("{why}; attempt {next}");
        self.enter(RunStage::Building, &reason, Some(feedback), now, effects);
    }

    /// Stops for the person; a retry goes back to the stage it stopped in,
    /// except that one stopped publishing checks again first: the branch
    /// may have gained commits elsewhere since (which were not checked, and
    /// which a push would have to take in).
    fn needs_you(&mut self, reason: String, now: Instant, effects: &mut Vec<RunEffect>) {
        if self.stage != RunStage::NeedsYou {
            self.resume = Some(match self.stage {
                RunStage::OpeningPr => RunStage::Checking,
                stage => stage,
            });
        }
        self.transition(RunStage::NeedsYou, reason.clone(), now, effects);
        self.reason = Some(reason);
    }

    /// Goes back to work where the run stopped, or to a new container if
    /// it has none; the building agent is told the person's notes, else
    /// what it was told last.
    fn retry(&mut self, notes: Option<String>, now: Instant, effects: &mut Vec<RunEffect>) {
        self.fresh_budget();
        let notes = notes
            .filter(|notes| !notes.trim().is_empty())
            .map(|notes| Feedback::PersonNotes { notes });
        let resume = match self.resume.take() {
            _ if !self.has_container => RunStage::Provisioning,
            Some(stage) => stage,
            None => RunStage::Building,
        };
        let reason = "you sent it back to work";
        match resume {
            RunStage::Building => {
                let feedback = notes.or_else(|| self.last_feedback.clone());
                self.enter(RunStage::Building, reason, feedback, now, effects);
            }
            RunStage::Provisioning => {
                self.pending_feedback = notes;
                self.enter(RunStage::Provisioning, reason, None, now, effects);
            }
            stage => self.enter(stage, reason, None, now, effects),
        }
    }

    fn cancel(&mut self, now: Instant, effects: &mut Vec<RunEffect>) {
        self.stop_work(effects);
        self.end(RunStage::Cancelled, "you cancelled it", now, effects);
    }

    /// Asks for whatever runs to stop.
    fn stop_work(&self, effects: &mut Vec<RunEffect>) {
        if self.stage.is_active() || self.stage == RunStage::TakenOver {
            effects.push(STOP_WORK);
        }
    }

    fn end(&mut self, stage: RunStage, reason: &str, now: Instant, effects: &mut Vec<RunEffect>) {
        // A container may exist from the moment one was asked for, ready
        // or not: one still starting, or one whose checkout failed.
        let may_have_container = self.has_container || self.generation > 0;
        self.transition(stage, reason.to_owned(), now, effects);
        if may_have_container {
            effects.push(RunEffect::RemoveContainer);
            self.has_container = false;
        }
    }

    fn fresh_budget(&mut self) {
        self.attempts_left = self.limits.attempts;
        self.active_spent = Duration::ZERO;
        self.cost_at_budget = self.cost_micros;
    }

    /// Records the move to `stage`, first of the effects, and keeps the
    /// time budget: only stretches of work on its own count.
    fn transition(
        &mut self,
        stage: RunStage,
        reason: String,
        now: Instant,
        effects: &mut Vec<RunEffect>,
    ) {
        let from = self.stage;
        if let Some(since) = self.active_since.take() {
            self.active_spent += now.saturating_duration_since(since);
        }
        if stage.is_active() {
            self.active_since = Some(now);
        }
        if stage != RunStage::NeedsYou {
            self.reason = None;
        }
        self.stage = stage;
        self.generation += 1;
        // Stored before any of the work it brings (`ARC-10`); a step makes
        // at most one transition.
        effects.insert(
            0,
            RunEffect::Transition {
                from,
                to: stage,
                reason,
            },
        );
    }
}

/// Stopping work, its generation filled in at the end of the step.
const STOP_WORK: RunEffect = RunEffect::StopWork { before: 0 };

/// The generation a report names, for reports on work asked for.
fn generation_of(event: &RunEvent) -> Option<u64> {
    match event {
        RunEvent::ContainerReady { generation }
        | RunEvent::AgentFinished { generation, .. }
        | RunEvent::ChecksFinished { generation, .. }
        | RunEvent::PrOpened { generation, .. }
        | RunEvent::ReviewSubmitted { generation, .. }
        | RunEvent::InfraFailed { generation, .. } => Some(*generation),
        _ => None,
    }
}

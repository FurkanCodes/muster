//! The session status machine.

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use muster_core::{SessionStatus, WaitReason};

use super::signal::{Effect, HookEvent, Origin, Signal};

mod carry;

pub use carry::Carried;

/// The time windows the machine uses.
///
/// The defaults come from Nebula's experience with Claude Code. Re-measure
/// them against the current CLI before relying on them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timing {
    /// After the main agent has ended its turn and its last subagent has
    /// stopped, how long to keep showing `Running` in case the main agent
    /// picks the results up and carries on.
    pub drain_grace: Duration,
    /// After the main agent has ended its turn, how long its subagents may
    /// stay silent before they are presumed gone.
    pub subagent_quiet: Duration,
    /// A subagent starting this soon after a finish shows the finish was
    /// premature.
    pub heal_window: Duration,
    /// A deferred permission notice arriving this soon after the person
    /// answered is an echo of the prompt they already answered.
    pub echo_window: Duration,
    /// How long a harness that reports its start may take to do so before
    /// it is taken to be asking the person something first. Claude Code
    /// 2.1.296 reports within a second; it says nothing while its trust
    /// question, a sign-in or first-run setup is on screen.
    pub start_grace: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            drain_grace: Duration::from_secs(180),
            subagent_quiet: Duration::from_secs(30 * 60),
            heal_window: Duration::from_secs(30),
            echo_window: Duration::from_secs(5),
            start_grace: Duration::from_secs(10),
        }
    }
}

/// How a session was launched.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Launch {
    /// The harness's own session id, when Muster chose it at launch
    /// (Claude Code's `--session-id`) or resumed it (`--resume`).
    pub agent_session: Option<String>,
    pub start: Start,
    /// Starts with something the person has not seen: a turn a daemon
    /// restart cut short, or a resume that failed and started over.
    pub unseen: bool,
    /// The harness reports through hooks, the first of them as it starts;
    /// until one arrives it has not started.
    pub reports_start: bool,
}

/// What the agent is doing as it starts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Start {
    /// Waiting for its first prompt.
    #[default]
    Idle,
    /// Given a first prompt at launch, so working from the start.
    WithPrompt,
    /// Resumed: back at its prompt with its earlier conversation.
    Resumed,
}

/// The main agent ended its turn while subagents were still working, so the
/// session keeps its status until they are done.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Hold {
    since: Instant,
    /// When the last subagent stopped, if none is running now.
    drained_at: Option<Instant>,
}

/// Decides one session's status from the signals it emits.
///
/// Pure: no IO, and time comes in with every call (`ARC-01`). The daemon
/// feeds it signals, carries out the [`Effect`]s it returns, and calls it with
/// [`Signal::Tick`] at [`next_deadline`](Self::next_deadline).
#[derive(Clone, Debug)]
pub struct SessionMachine {
    timing: Timing,
    status: SessionStatus,
    unseen: bool,
    /// Someone is watching the terminal, so nothing is unseen.
    watched: bool,
    /// The harness session this process is running; hooks from any other are ignored.
    agent_session: Option<String>,
    /// The id last handed to the daemon for resuming.
    resume_id: Option<String>,
    /// Open prompts waiting on the person, oldest first.
    waits: Vec<(Origin, WaitReason)>,
    /// The last prompt the person answered, for echo suppression.
    answered: Option<(Origin, Instant)>,
    subagents: BTreeSet<String>,
    subagent_activity: Option<Instant>,
    hold: Option<Hold>,
    finished_at: Option<Instant>,
    /// Whether the progress sequence has gone busy since the last finish.
    busy_seen: bool,
    /// When the harness must have reported its start by, until it has.
    start_by: Option<Instant>,
    /// The status and unseen mark from before the harness was taken to be
    /// asking something first, to go back to once it starts: a start that
    /// was only slow leaves no news behind, and whoever answered a question
    /// was watching anyway.
    before_startup: Option<(SessionStatus, bool)>,
}

impl SessionMachine {
    /// A machine for a session launched at `now`.
    pub fn new(launch: Launch, timing: Timing, now: Instant) -> Self {
        let status = match launch.start {
            Start::Idle => SessionStatus::Fresh,
            Start::WithPrompt => SessionStatus::Running,
            Start::Resumed => SessionStatus::Finished,
        };
        // A resumed session's id already has a transcript behind it.
        let resume_id = match launch.start {
            Start::Resumed => launch.agent_session.clone(),
            _ => None,
        };
        Self {
            timing,
            status,
            unseen: launch.unseen,
            watched: false,
            agent_session: launch.agent_session,
            resume_id,
            waits: Vec::new(),
            answered: None,
            subagents: BTreeSet::new(),
            subagent_activity: None,
            hold: None,
            finished_at: None,
            busy_seen: false,
            start_by: launch.reports_start.then(|| now + timing.start_grace),
            before_startup: None,
        }
    }

    /// The current status.
    pub fn status(&self) -> SessionStatus {
        self.status
    }

    /// Whether something happened that the person has not looked at yet.
    pub fn unseen(&self) -> bool {
        self.unseen
    }

    /// The harness session this process is running, once known.
    pub fn agent_session(&self) -> Option<&str> {
        self.agent_session.as_deref()
    }

    /// When the daemon should next send [`Signal::Tick`], if ever.
    pub fn next_deadline(&self) -> Option<Instant> {
        let start = self.start_by.filter(|_| self.before_startup.is_none());
        match (start, self.hold_deadline()) {
            (Some(start), Some(hold)) => Some(start.min(hold)),
            (start, hold) => start.or(hold),
        }
    }

    fn hold_deadline(&self) -> Option<Instant> {
        let hold = self.hold?;
        if let Some(drained_at) = hold.drained_at {
            return Some(drained_at + self.timing.drain_grace);
        }
        if self.is_waiting() {
            // A subagent waiting on the person is not quiet, however long it takes.
            return None;
        }
        Some(self.last_subagent_activity(hold) + self.timing.subagent_quiet)
    }

    /// Applies one signal and returns what the daemon must do about it.
    pub fn step(&mut self, signal: Signal, now: Instant) -> Vec<Effect> {
        let before = (self.status, self.unseen);
        let mut effects = Vec::new();

        if let Signal::Watched { watching } = signal {
            self.watched = watching;
        } else if !self.status.is_exited() {
            // Nothing changes an exited session except being looked at.
            match signal {
                Signal::Hook {
                    agent_session,
                    event,
                } => {
                    // Any word from the harness means it has started.
                    self.started();
                    self.on_hook(agent_session, event, now, &mut effects);
                }
                Signal::Progress { busy } => self.on_progress(busy, now),
                Signal::Waiting { waiting } => {
                    // Its display is up: it has started.
                    self.started();
                    if waiting {
                        self.wait_for(Origin::Main, WaitReason::Action);
                    } else {
                        self.answer(&Origin::Main, WaitReason::Action, now);
                    }
                }
                Signal::Exited { success, requested } => self.on_exit(success, requested),
                Signal::Tick => self.on_tick(now),
                Signal::Watched { .. } => {}
            }
        }
        if self.watched {
            self.unseen = false;
        }

        if (self.status, self.unseen) != before {
            effects.push(Effect::Status {
                status: self.status,
                unseen: self.unseen,
            });
        }
        effects
    }

    fn on_hook(
        &mut self,
        agent_session: Option<String>,
        event: HookEvent,
        now: Instant,
        effects: &mut Vec<Effect>,
    ) {
        if let Some(id) = agent_session {
            match &self.agent_session {
                // `/clear` starts a new harness session in the same process;
                // only its start announcement may switch to it.
                Some(current) if *current != id && event != HookEvent::SessionStarted => return,
                _ => self.agent_session = Some(id),
            }
        }

        match event {
            HookEvent::SessionStarted => {}
            HookEvent::PromptSubmitted => {
                self.save_resume_id(effects);
                self.start_turn();
            }
            HookEvent::TurnEnded => {
                self.save_resume_id(effects);
                self.end_turn(now);
            }
            HookEvent::PermissionRequested { origin, deferred } => {
                self.on_permission(origin, deferred, now);
            }
            HookEvent::QuestionAsked { origin } => {
                self.note_activity(&origin, now);
                self.wait_for(origin, WaitReason::Question);
            }
            HookEvent::ToolStarted { origin } => self.on_tool_started(origin, now),
            HookEvent::ToolFinished { origin, question } => {
                self.note_activity(&origin, now);
                let reason = if question {
                    WaitReason::Question
                } else {
                    WaitReason::Permission
                };
                self.answer(&origin, reason, now);
            }
            HookEvent::Idle => {
                // Only a turn end the other signals missed; never one held for subagents.
                if self.status == SessionStatus::Running && self.hold.is_none() {
                    self.end_turn(now);
                }
            }
            HookEvent::SubagentStarted { id } => self.on_subagent_started(id, now),
            HookEvent::SubagentStopped { id } => self.on_subagent_stopped(&id, now),
        }
    }

    fn on_permission(&mut self, origin: Origin, deferred: bool, now: Instant) {
        self.note_activity(&origin, now);
        if deferred {
            let echo = self.answered.as_ref().is_some_and(|(answered, at)| {
                *answered == origin && now.duration_since(*at) < self.timing.echo_window
            });
            // A trailing notice can only add a prompt nobody has seen yet, and
            // only while a turn is running.
            if echo || self.is_waiting() || self.status != SessionStatus::Running {
                return;
            }
        }
        self.wait_for(origin, WaitReason::Permission);
    }

    fn on_tool_started(&mut self, origin: Origin, now: Instant) {
        self.note_activity(&origin, now);
        self.answer(&origin, WaitReason::Permission, now);
        if origin == Origin::Main {
            // The main agent is working again, so its turn has not ended.
            self.hold = None;
            if matches!(self.status, SessionStatus::Fresh | SessionStatus::Finished) {
                self.resume_running();
            }
        }
    }

    fn on_subagent_started(&mut self, id: String, now: Instant) {
        self.subagents.insert(id);
        self.subagent_activity = Some(now);
        if let Some(hold) = &mut self.hold {
            hold.drained_at = None;
        }
        let recent_finish = self
            .finished_at
            .is_some_and(|at| now.duration_since(at) < self.timing.heal_window);
        if self.status == SessionStatus::Finished && recent_finish {
            // The finish was premature: the main agent's turn ended while this
            // subagent was on its way.
            self.resume_running();
            self.hold = Some(Hold {
                since: now,
                drained_at: None,
            });
        }
    }

    fn on_subagent_stopped(&mut self, id: &str, now: Instant) {
        self.subagents.remove(id);
        self.subagent_activity = Some(now);
        // A stopped subagent can no longer be waiting on anyone.
        let origin = Origin::Subagent(id.to_owned());
        self.waits.retain(|(waiting, _)| *waiting != origin);
        self.refresh_waiting();
        if let Some(hold) = &mut self.hold
            && self.subagents.is_empty()
        {
            hold.drained_at = Some(now);
        }
    }

    fn on_progress(&mut self, busy: bool, now: Instant) {
        if busy {
            self.busy_seen = true;
            if matches!(self.status, SessionStatus::Fresh | SessionStatus::Finished) {
                self.resume_running();
            }
        } else if self.status == SessionStatus::Running && self.busy_seen && self.hold.is_none() {
            // Idle without an earlier busy edge is the startup state, not a
            // finished turn. After a person cancels a turn this is the only
            // signal that it ended.
            self.end_turn(now);
        }
    }

    fn on_exit(&mut self, success: bool, requested: bool) {
        self.start_by = None;
        self.before_startup = None;
        self.status = SessionStatus::Exited { success };
        self.unseen = !requested;
        self.waits.clear();
        self.subagents.clear();
        self.hold = None;
    }

    /// The harness reported in: it was not asking before it started, or
    /// has been answered.
    fn started(&mut self) {
        self.start_by = None;
        if let Some((status, unseen)) = self.before_startup.take()
            && self.status
                == (SessionStatus::NeedsYou {
                    reason: WaitReason::Startup,
                })
        {
            self.status = status;
            self.unseen = unseen;
        }
    }

    fn on_tick(&mut self, now: Instant) {
        if let Some(start_by) = self.start_by
            && self.before_startup.is_none()
            && now >= start_by
            && !self.is_waiting()
        {
            // Silent since launch: it is asking the person something first.
            self.before_startup = Some((self.status, self.unseen));
            self.status = SessionStatus::NeedsYou {
                reason: WaitReason::Startup,
            };
            self.unseen = true;
        }
        let Some(hold) = self.hold else {
            return;
        };
        if let Some(drained_at) = hold.drained_at {
            if now >= drained_at + self.timing.drain_grace {
                self.finish(now);
            }
        } else if !self.is_waiting()
            && now >= self.last_subagent_activity(hold) + self.timing.subagent_quiet
        {
            self.subagents.clear();
            self.finish(now);
        }
    }

    fn start_turn(&mut self) {
        self.waits.clear();
        self.hold = None;
        self.resume_running();
    }

    fn end_turn(&mut self, now: Instant) {
        if !matches!(
            self.status,
            SessionStatus::Running | SessionStatus::NeedsYou { .. }
        ) {
            return;
        }
        // The main agent's own prompts close with its turn.
        self.waits.retain(|(origin, _)| *origin != Origin::Main);
        self.refresh_waiting();
        if self.subagents.is_empty() {
            self.finish(now);
        } else if self.hold.is_none() {
            self.hold = Some(Hold {
                since: now,
                drained_at: None,
            });
        }
    }

    fn finish(&mut self, now: Instant) {
        self.status = SessionStatus::Finished;
        self.unseen = true;
        self.finished_at = Some(now);
        self.waits.clear();
        self.hold = None;
        self.busy_seen = false;
    }

    fn resume_running(&mut self) {
        self.status = SessionStatus::Running;
        self.unseen = false;
    }

    fn wait_for(&mut self, origin: Origin, reason: WaitReason) {
        if !self.waits.contains(&(origin.clone(), reason)) {
            self.waits.push((origin, reason));
        }
        self.unseen = true;
        self.refresh_waiting();
    }

    /// Closes `origin`'s open prompts of `reason`, if any.
    fn answer(&mut self, origin: &Origin, reason: WaitReason, now: Instant) {
        let open = self.waits.len();
        self.waits
            .retain(|(waiting, waiting_reason)| !(waiting == origin && *waiting_reason == reason));
        if self.waits.len() != open {
            self.answered = Some((origin.clone(), now));
            self.refresh_waiting();
        }
    }

    /// Brings the status in line with the open prompts: the newest one is
    /// shown; with none left, the session is running again.
    fn refresh_waiting(&mut self) {
        match self.waits.last() {
            Some((_, reason)) => self.status = SessionStatus::NeedsYou { reason: *reason },
            None if self.is_waiting() => self.resume_running(),
            None => {}
        }
    }

    fn is_waiting(&self) -> bool {
        matches!(self.status, SessionStatus::NeedsYou { .. })
    }

    fn note_activity(&mut self, origin: &Origin, now: Instant) {
        if matches!(origin, Origin::Subagent(_)) {
            self.subagent_activity = Some(now);
        }
    }

    fn last_subagent_activity(&self, hold: Hold) -> Instant {
        self.subagent_activity
            .map_or(hold.since, |activity| activity.max(hold.since))
    }

    /// Hands the current harness session to the daemon for resuming, once a
    /// turn has run in it, so the id never points at an empty transcript.
    fn save_resume_id(&mut self, effects: &mut Vec<Effect>) {
        if let Some(id) = &self.agent_session
            && self.resume_id.as_ref() != Some(id)
        {
            self.resume_id = Some(id.clone());
            effects.push(Effect::SaveResumeId(id.clone()));
        }
    }
}

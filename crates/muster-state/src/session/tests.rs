#![expect(clippy::disallowed_methods, reason = "tests time themselves")]

use std::time::{Duration, Instant};

use muster_core::{SessionStatus, WaitReason};

use super::{Effect, HookEvent, Launch, Origin, SessionMachine, Signal, Start, Timing};

const NEEDS_PERMISSION: SessionStatus = SessionStatus::NeedsYou {
    reason: WaitReason::Permission,
};
const NEEDS_ANSWER: SessionStatus = SessionStatus::NeedsYou {
    reason: WaitReason::Question,
};

/// A machine with a fake clock that only moves when told to.
struct Session {
    machine: SessionMachine,
    start: Instant,
    now: Instant,
}

impl Session {
    fn fresh() -> Self {
        Self::launched(Launch::default())
    }

    fn launched(launch: Launch) -> Self {
        let start = Instant::now();
        Self {
            machine: SessionMachine::new(launch, Timing::default(), start),
            start,
            now: start,
        }
    }

    fn at(&mut self, seconds: u64) -> &mut Self {
        self.now = self.start + Duration::from_secs(seconds);
        self
    }

    fn send(&mut self, signal: Signal) -> Vec<Effect> {
        self.machine.step(signal, self.now)
    }

    fn hook(&mut self, event: HookEvent) -> Vec<Effect> {
        self.send(Signal::Hook {
            agent_session: None,
            event,
        })
    }

    fn hook_from(&mut self, agent_session: &str, event: HookEvent) -> Vec<Effect> {
        self.send(Signal::Hook {
            agent_session: Some(agent_session.to_owned()),
            event,
        })
    }

    fn status(&self) -> SessionStatus {
        self.machine.status()
    }

    fn unseen(&self) -> bool {
        self.machine.unseen()
    }

    fn running_turn() -> Self {
        let mut session = Self::fresh();
        session.hook(HookEvent::PromptSubmitted);
        session
    }
}

fn sub(id: &str) -> Origin {
    Origin::Subagent(id.to_owned())
}

fn permission(origin: Origin) -> HookEvent {
    HookEvent::PermissionRequested {
        origin,
        deferred: false,
    }
}

fn deferred_permission(origin: Origin) -> HookEvent {
    HookEvent::PermissionRequested {
        origin,
        deferred: true,
    }
}

fn tool_started(origin: Origin) -> HookEvent {
    HookEvent::ToolStarted { origin }
}

fn subagent_started(id: &str) -> HookEvent {
    HookEvent::SubagentStarted { id: id.to_owned() }
}

fn subagent_stopped(id: &str) -> HookEvent {
    HookEvent::SubagentStopped { id: id.to_owned() }
}

fn status_effect(status: SessionStatus, unseen: bool) -> Effect {
    Effect::Status { status, unseen }
}

// Launch

#[test]
fn a_launch_without_a_prompt_is_fresh_and_seen() {
    let session = Session::fresh();
    assert_eq!(session.status(), SessionStatus::Fresh);
    assert!(!session.unseen());
    assert_eq!(session.machine.next_deadline(), None);
}

#[test]
fn a_launch_with_a_prompt_is_running() {
    let session = Session::launched(Launch {
        agent_session: None,
        start: Start::WithPrompt,
        unseen: false,
        reports_start: false,
    });
    assert_eq!(session.status(), SessionStatus::Running);
}

// Turns

#[test]
fn a_finished_turn_is_unseen_until_watched() {
    let mut session = Session::fresh();
    assert_eq!(
        session.hook(HookEvent::PromptSubmitted),
        [status_effect(SessionStatus::Running, false)]
    );
    assert_eq!(
        session.hook(HookEvent::TurnEnded),
        [status_effect(SessionStatus::Finished, true)]
    );
    assert_eq!(
        session.send(Signal::Watched { watching: true }),
        [status_effect(SessionStatus::Finished, false)]
    );
}

#[test]
fn nothing_is_unseen_while_the_session_is_watched() {
    let mut session = Session::fresh();
    let watch = |watching| Signal::Watched { watching };
    assert!(session.send(watch(true)).is_empty());
    session.hook(HookEvent::PromptSubmitted);
    assert_eq!(
        session.hook(HookEvent::TurnEnded),
        [status_effect(SessionStatus::Finished, false)]
    );
    session.hook(HookEvent::PromptSubmitted);
    assert_eq!(
        session.hook(permission(Origin::Main)),
        [status_effect(NEEDS_PERMISSION, false)]
    );
    session.hook(tool_started(Origin::Main));
    session.hook(HookEvent::QuestionAsked {
        origin: Origin::Main,
    });
    assert_eq!(session.status(), NEEDS_ANSWER);
    assert!(!session.unseen());
    assert_eq!(
        session.send(Signal::Exited {
            success: false,
            requested: false,
        }),
        [status_effect(
            SessionStatus::Exited { success: false },
            false
        )]
    );
}

#[test]
fn news_after_watching_stops_is_unseen_again() {
    let mut session = Session::running_turn();
    session.send(Signal::Watched { watching: true });
    // Stopping watching changes nothing by itself.
    assert!(session.send(Signal::Watched { watching: false }).is_empty());
    assert_eq!(
        session.hook(HookEvent::TurnEnded),
        [status_effect(SessionStatus::Finished, true)]
    );
}

#[test]
fn a_resumed_session_with_news_is_seen_once_watched() {
    let mut session = Session::launched(Launch {
        agent_session: Some("kept".to_owned()),
        start: Start::Resumed,
        unseen: true,
        reports_start: false,
    });
    assert_eq!(
        session.send(Signal::Watched { watching: true }),
        [status_effect(SessionStatus::Finished, false)]
    );
}

#[test]
fn a_launch_with_news_keeps_it_until_watched() {
    let mut session = Session::launched(Launch {
        agent_session: None,
        start: Start::Idle,
        unseen: true,
        reports_start: false,
    });
    assert_eq!(
        (session.status(), session.unseen()),
        (SessionStatus::Fresh, true)
    );
    assert_eq!(
        session.send(Signal::Watched { watching: true }),
        [status_effect(SessionStatus::Fresh, false)]
    );
}

#[test]
fn signals_that_change_nothing_produce_no_effects() {
    let mut session = Session::running_turn();
    session.hook(HookEvent::TurnEnded);
    assert!(session.hook(HookEvent::TurnEnded).is_empty());
    assert!(session.send(Signal::Tick).is_empty());
    assert!(session.hook(HookEvent::SessionStarted).is_empty());
}

#[test]
fn a_main_tool_after_a_finish_means_a_turn_the_prompt_hook_missed() {
    let mut session = Session::running_turn();
    session.hook(HookEvent::TurnEnded);
    session.hook(tool_started(Origin::Main));
    assert_eq!(session.status(), SessionStatus::Running);
    assert!(!session.unseen());
}

#[test]
fn an_idle_report_ends_a_turn_the_stop_hook_missed() {
    let mut session = Session::running_turn();
    session.hook(HookEvent::Idle);
    assert_eq!(session.status(), SessionStatus::Finished);
}

#[test]
fn an_idle_report_does_not_end_a_turn_held_for_subagents() {
    let mut session = Session::running_turn();
    session.hook(subagent_started("a"));
    session.hook(HookEvent::TurnEnded);
    session.hook(HookEvent::Idle);
    assert_eq!(session.status(), SessionStatus::Running);
}

// Permission prompts and questions

#[test]
fn a_permission_prompt_needs_you_until_the_tool_starts() {
    let mut session = Session::running_turn();
    assert_eq!(
        session.hook(permission(Origin::Main)),
        [status_effect(NEEDS_PERMISSION, true)]
    );
    assert_eq!(
        session.hook(tool_started(Origin::Main)),
        [status_effect(SessionStatus::Running, false)]
    );
}

#[test]
fn a_permission_prompt_is_not_answered_by_another_origins_tool() {
    let mut session = Session::running_turn();
    session.hook(subagent_started("a"));
    session.hook(permission(Origin::Main));
    session.hook(tool_started(sub("a")));
    assert_eq!(session.status(), NEEDS_PERMISSION);
}

#[test]
fn a_question_is_answered_only_by_its_own_completion() {
    let mut session = Session::running_turn();
    session.hook(HookEvent::QuestionAsked {
        origin: Origin::Main,
    });
    assert_eq!(session.status(), NEEDS_ANSWER);

    session.hook(tool_started(Origin::Main));
    session.hook(HookEvent::ToolFinished {
        origin: Origin::Main,
        question: false,
    });
    assert_eq!(session.status(), NEEDS_ANSWER);

    session.hook(HookEvent::ToolFinished {
        origin: Origin::Main,
        question: true,
    });
    assert_eq!(session.status(), SessionStatus::Running);
}

#[test]
fn a_question_announced_twice_is_answered_once() {
    // Claude Code's question tool raises its PreToolUse and then its own
    // permission dialog, both read as the question.
    let mut session = Session::running_turn();
    let asked = || HookEvent::QuestionAsked {
        origin: Origin::Main,
    };
    session.hook(asked());
    assert_eq!(session.hook(asked()), []);
    session.hook(HookEvent::ToolFinished {
        origin: Origin::Main,
        question: true,
    });
    assert_eq!(session.status(), SessionStatus::Running);
}

#[test]
fn several_open_prompts_keep_the_session_waiting_until_all_are_answered() {
    let mut session = Session::running_turn();
    session.hook(subagent_started("a"));
    session.hook(HookEvent::QuestionAsked { origin: sub("a") });
    session.hook(permission(Origin::Main));
    assert_eq!(session.status(), NEEDS_PERMISSION);

    // Answering the newest leaves the older one showing.
    session.hook(tool_started(Origin::Main));
    assert_eq!(session.status(), NEEDS_ANSWER);

    session.hook(HookEvent::ToolFinished {
        origin: sub("a"),
        question: true,
    });
    assert_eq!(session.status(), SessionStatus::Running);
}

#[test]
fn a_deferred_notice_just_after_an_answer_is_an_echo() {
    let mut session = Session::running_turn();
    session.at(10).hook(permission(Origin::Main));
    session.at(12).hook(tool_started(Origin::Main));
    session.at(16).hook(deferred_permission(Origin::Main));
    assert_eq!(session.status(), SessionStatus::Running);
}

#[test]
fn a_deferred_notice_after_the_echo_window_is_a_new_prompt() {
    let mut session = Session::running_turn();
    session.at(10).hook(permission(Origin::Main));
    session.at(12).hook(tool_started(Origin::Main));
    session.at(18).hook(deferred_permission(Origin::Main));
    assert_eq!(session.status(), NEEDS_PERMISSION);
}

#[test]
fn a_real_request_right_after_an_answer_is_never_taken_for_an_echo() {
    let mut session = Session::running_turn();
    session.at(10).hook(permission(Origin::Main));
    session.at(11).hook(tool_started(Origin::Main));
    session.at(12).hook(permission(Origin::Main));
    assert_eq!(session.status(), NEEDS_PERMISSION);
}

#[test]
fn a_deferred_notice_after_the_turn_ended_is_ignored() {
    let mut session = Session::running_turn();
    session.hook(HookEvent::TurnEnded);
    session.at(60).hook(deferred_permission(Origin::Main));
    assert_eq!(session.status(), SessionStatus::Finished);
}

#[test]
fn ending_the_turn_closes_the_main_agents_prompts() {
    let mut session = Session::running_turn();
    session.hook(permission(Origin::Main));
    session.hook(HookEvent::TurnEnded);
    assert_eq!(session.status(), SessionStatus::Finished);
}

#[test]
fn a_new_prompt_closes_every_open_prompt() {
    let mut session = Session::running_turn();
    session.hook(permission(Origin::Main));
    session.hook(HookEvent::PromptSubmitted);
    assert_eq!(session.status(), SessionStatus::Running);
}

// Subagents

#[test]
fn a_turn_end_waits_for_subagents_and_then_a_grace_period() {
    let mut session = Session::running_turn();
    session.at(0).hook(subagent_started("a"));
    session.at(10).hook(HookEvent::TurnEnded);
    assert_eq!(session.status(), SessionStatus::Running);

    session.at(100).hook(subagent_stopped("a"));
    let finish_at = session.start + Duration::from_secs(280);
    assert_eq!(session.machine.next_deadline(), Some(finish_at));

    session.at(279).send(Signal::Tick);
    assert_eq!(session.status(), SessionStatus::Running);
    session.at(280).send(Signal::Tick);
    assert_eq!(session.status(), SessionStatus::Finished);
    assert!(session.unseen());
}

#[test]
fn a_subagent_starting_during_the_grace_period_cancels_it() {
    let mut session = Session::running_turn();
    session.at(0).hook(subagent_started("a"));
    session.at(10).hook(HookEvent::TurnEnded);
    session.at(20).hook(subagent_stopped("a"));
    session.at(30).hook(subagent_started("b"));
    session.at(500).send(Signal::Tick);
    assert_eq!(session.status(), SessionStatus::Running);
}

#[test]
fn the_main_agent_carrying_on_cancels_the_hold() {
    let mut session = Session::running_turn();
    session.at(0).hook(subagent_started("a"));
    session.at(10).hook(HookEvent::TurnEnded);
    session.at(20).hook(tool_started(Origin::Main));
    assert_eq!(session.machine.next_deadline(), None);

    session.at(30).hook(subagent_stopped("a"));
    session.at(40).hook(HookEvent::TurnEnded);
    assert_eq!(session.status(), SessionStatus::Finished);
}

#[test]
fn silent_subagents_are_presumed_gone() {
    let mut session = Session::running_turn();
    session.at(0).hook(subagent_started("a"));
    session.at(10).hook(HookEvent::TurnEnded);
    session.at(60).hook(tool_started(sub("a")));
    let quiet_at = session.start + Duration::from_mins(31);
    assert_eq!(session.machine.next_deadline(), Some(quiet_at));

    session.at(60 + 30 * 60).send(Signal::Tick);
    assert_eq!(session.status(), SessionStatus::Finished);
}

#[test]
fn a_subagent_waiting_on_the_person_is_never_presumed_gone() {
    let mut session = Session::running_turn();
    session.at(0).hook(subagent_started("a"));
    session.at(10).hook(HookEvent::TurnEnded);
    session.at(20).hook(permission(sub("a")));
    assert_eq!(session.machine.next_deadline(), None);

    session.at(10 * 60 * 60).send(Signal::Tick);
    assert_eq!(session.status(), NEEDS_PERMISSION);
}

#[test]
fn a_stopped_subagent_closes_its_own_prompts() {
    let mut session = Session::running_turn();
    session.hook(subagent_started("a"));
    session.hook(HookEvent::QuestionAsked { origin: sub("a") });
    session.hook(subagent_stopped("a"));
    assert_eq!(session.status(), SessionStatus::Running);
}

#[test]
fn a_subagent_soon_after_a_finish_heals_it() {
    let mut session = Session::running_turn();
    session.at(10).hook(HookEvent::TurnEnded);
    session.at(20).hook(subagent_started("a"));
    assert_eq!(session.status(), SessionStatus::Running);
    assert!(!session.unseen());

    session.at(30).hook(subagent_stopped("a"));
    session.at(30 + 180).send(Signal::Tick);
    assert_eq!(session.status(), SessionStatus::Finished);
}

#[test]
fn a_subagent_long_after_a_finish_does_not_heal_it() {
    let mut session = Session::running_turn();
    session.at(10).hook(HookEvent::TurnEnded);
    session.at(41).hook(subagent_started("a"));
    assert_eq!(session.status(), SessionStatus::Finished);
}

// Progress sequence

#[test]
fn progress_alone_drives_a_harness_without_hooks() {
    let mut session = Session::fresh();
    session.send(Signal::Progress { busy: true });
    assert_eq!(session.status(), SessionStatus::Running);
    session.send(Signal::Progress { busy: false });
    assert_eq!(session.status(), SessionStatus::Finished);
}

#[test]
fn an_idle_edge_before_any_busy_edge_is_the_startup_state() {
    let mut session = Session::launched(Launch {
        agent_session: None,
        start: Start::WithPrompt,
        unseen: false,
        reports_start: false,
    });
    session.send(Signal::Progress { busy: false });
    assert_eq!(session.status(), SessionStatus::Running);
}

#[test]
fn an_idle_edge_ends_a_cancelled_turn() {
    // A cancelled turn fires no stop hook.
    let mut session = Session::running_turn();
    session.send(Signal::Progress { busy: true });
    session.send(Signal::Progress { busy: false });
    assert_eq!(session.status(), SessionStatus::Finished);
}

#[test]
fn an_idle_edge_does_not_close_an_open_prompt() {
    let mut session = Session::running_turn();
    session.send(Signal::Progress { busy: true });
    session.hook(permission(Origin::Main));
    session.send(Signal::Progress { busy: false });
    assert_eq!(session.status(), NEEDS_PERMISSION);
}

#[test]
fn a_busy_edge_does_not_cancel_a_hold() {
    let mut session = Session::running_turn();
    session.at(0).hook(subagent_started("a"));
    session.at(10).hook(HookEvent::TurnEnded);
    session.at(11).send(Signal::Progress { busy: true });
    session.at(20).hook(subagent_stopped("a"));
    session.at(200).send(Signal::Tick);
    assert_eq!(session.status(), SessionStatus::Finished);
}

// Harness sessions

#[test]
fn hooks_from_another_harness_session_are_ignored() {
    let mut session = Session::launched(Launch {
        agent_session: Some("ours".to_owned()),
        start: Start::Idle,
        unseen: false,
        reports_start: false,
    });
    assert!(
        session
            .hook_from("theirs", HookEvent::PromptSubmitted)
            .is_empty()
    );
    assert_eq!(session.status(), SessionStatus::Fresh);
    assert_eq!(session.machine.agent_session(), Some("ours"));
}

#[test]
fn clear_switches_to_the_new_harness_session() {
    let mut session = Session::launched(Launch {
        agent_session: Some("first".to_owned()),
        start: Start::Idle,
        unseen: false,
        reports_start: false,
    });
    session.hook_from("second", HookEvent::SessionStarted);
    assert_eq!(session.machine.agent_session(), Some("second"));
    session.hook_from("second", HookEvent::PromptSubmitted);
    assert_eq!(session.status(), SessionStatus::Running);
    session.hook_from("first", HookEvent::TurnEnded);
    assert_eq!(session.status(), SessionStatus::Running);
}

#[test]
fn the_first_hook_names_the_harness_session_when_launch_did_not() {
    let mut session = Session::fresh();
    session.hook_from("theirs", HookEvent::PromptSubmitted);
    assert_eq!(session.machine.agent_session(), Some("theirs"));
}

#[test]
fn the_resume_id_is_saved_once_a_turn_has_run() {
    let mut session = Session::launched(Launch {
        agent_session: Some("first".to_owned()),
        start: Start::Idle,
        unseen: false,
        reports_start: false,
    });
    assert!(
        session
            .hook_from("first", HookEvent::SessionStarted)
            .is_empty()
    );
    assert_eq!(
        session.hook_from("first", HookEvent::PromptSubmitted),
        [
            Effect::SaveResumeId("first".to_owned()),
            status_effect(SessionStatus::Running, false),
        ]
    );
    let effects = session.hook_from("first", HookEvent::TurnEnded);
    assert!(!effects.contains(&Effect::SaveResumeId("first".to_owned())));

    session.hook_from("second", HookEvent::SessionStarted);
    let effects = session.hook_from("second", HookEvent::PromptSubmitted);
    assert!(effects.contains(&Effect::SaveResumeId("second".to_owned())));
}

// Exit

#[test]
fn an_unexpected_exit_is_unseen() {
    let mut session = Session::running_turn();
    assert_eq!(
        session.send(Signal::Exited {
            success: false,
            requested: false,
        }),
        [status_effect(
            SessionStatus::Exited { success: false },
            true
        )]
    );
}

#[test]
fn a_requested_stop_is_not_unseen() {
    let mut session = Session::running_turn();
    session.send(Signal::Exited {
        success: true,
        requested: true,
    });
    assert_eq!(session.status(), SessionStatus::Exited { success: true });
    assert!(!session.unseen());
}

#[test]
fn nothing_but_watching_changes_an_exited_session() {
    let mut session = Session::running_turn();
    session.hook(subagent_started("a"));
    session.send(Signal::Exited {
        success: true,
        requested: false,
    });
    assert!(session.hook(HookEvent::PromptSubmitted).is_empty());
    assert!(session.send(Signal::Progress { busy: true }).is_empty());
    assert!(session.send(Signal::Tick).is_empty());
    assert_eq!(session.machine.next_deadline(), None);
    assert_eq!(
        session.send(Signal::Watched { watching: true }),
        [status_effect(
            SessionStatus::Exited { success: true },
            false
        )]
    );
}

#[test]
fn a_resumed_session_is_finished_and_keeps_what_was_unseen() {
    for unseen in [false, true] {
        let session = Session::launched(Launch {
            agent_session: Some("kept".to_owned()),
            start: Start::Resumed,
            unseen,
            reports_start: false,
        });
        assert_eq!(session.status(), SessionStatus::Finished);
        assert_eq!(session.unseen(), unseen);
    }
}

#[test]
fn a_resumed_session_does_not_save_its_id_again() {
    let mut session = Session::launched(Launch {
        agent_session: Some("kept".to_owned()),
        start: Start::Resumed,
        unseen: false,
        reports_start: false,
    });
    let effects = session.hook_from("kept", HookEvent::PromptSubmitted);
    assert!(!effects.contains(&Effect::SaveResumeId("kept".to_owned())));
}

// A harness that reports its start

fn reporting(start: Start) -> Session {
    Session::launched(Launch {
        agent_session: Some("s".to_owned()),
        start,
        unseen: false,
        reports_start: true,
    })
}

const STARTING: SessionStatus = SessionStatus::NeedsYou {
    reason: WaitReason::Startup,
};

#[test]
fn a_harness_silent_past_the_grace_is_asking_before_it_starts() {
    let mut session = reporting(Start::Idle);
    let grace = Timing::default().start_grace;
    assert_eq!(session.machine.next_deadline(), Some(session.start + grace));
    // Before the deadline, nothing.
    session.at(1);
    assert!(session.send(Signal::Tick).is_empty());
    session.now = session.start + grace;
    assert_eq!(session.send(Signal::Tick), [status_effect(STARTING, true)]);
    assert_eq!(session.machine.next_deadline(), None);
    // It starts: idle as launched, and no news if it was only slow.
    session.at(30);
    assert_eq!(
        session.hook_from("s", HookEvent::SessionStarted),
        [status_effect(SessionStatus::Fresh, false)]
    );
    assert_eq!(session.machine.next_deadline(), None);
}

#[test]
fn a_start_in_time_leaves_nothing_to_wait_for() {
    let mut session = reporting(Start::WithPrompt);
    session.at(1);
    assert!(session.hook_from("s", HookEvent::SessionStarted).is_empty());
    assert_eq!(session.machine.next_deadline(), None);
    session.at(60);
    assert!(session.send(Signal::Tick).is_empty());
    assert_eq!(session.status(), SessionStatus::Running);
}

#[test]
fn a_prompt_or_a_resume_goes_back_to_what_it_was_once_started() {
    let grace = Timing::default().start_grace;
    for (start, before) in [
        (Start::WithPrompt, SessionStatus::Running),
        (Start::Resumed, SessionStatus::Finished),
    ] {
        let mut session = reporting(start);
        session.now = session.start + grace;
        session.send(Signal::Tick);
        assert_eq!(session.status(), STARTING);
        // Any hook says it started, not only the start announcement.
        session.hook_from("s", subagent_stopped("none"));
        assert_eq!(session.status(), before, "{start:?}");
    }
    let mut session = reporting(Start::WithPrompt);
    session.now = session.start + grace;
    session.send(Signal::Tick);
    assert_eq!(
        session.hook_from("s", HookEvent::PromptSubmitted),
        [
            Effect::SaveResumeId("s".to_owned()),
            status_effect(SessionStatus::Running, false)
        ]
    );
}

#[test]
fn quitting_at_the_question_ends_the_wait() {
    let mut session = reporting(Start::Idle);
    session.now = session.start + Timing::default().start_grace;
    session.send(Signal::Tick);
    session.send(Signal::Exited {
        success: true,
        requested: false,
    });
    assert_eq!(session.status(), SessionStatus::Exited { success: true });
    assert_eq!(session.machine.next_deadline(), None);
}

#[test]
fn a_question_before_the_start_shown_to_someone_watching_is_not_news() {
    let mut session = reporting(Start::Idle);
    session.send(Signal::Watched { watching: true });
    session.now = session.start + Timing::default().start_grace;
    assert_eq!(session.send(Signal::Tick), [status_effect(STARTING, false)]);
}

#[test]
fn harnesses_that_do_not_report_their_start_are_never_waited_on() {
    let mut session = Session::fresh();
    assert_eq!(session.machine.next_deadline(), None);
    session.at(600);
    assert!(session.send(Signal::Tick).is_empty());
    assert_eq!(session.status(), SessionStatus::Fresh);
}

#[test]
fn news_from_before_a_slow_start_is_kept() {
    let mut session = Session::launched(Launch {
        agent_session: Some("s".to_owned()),
        start: Start::Resumed,
        unseen: true,
        reports_start: true,
    });
    session.now = session.start + Timing::default().start_grace;
    session.send(Signal::Tick);
    assert_eq!((session.status(), session.unseen()), (STARTING, true));
    session.hook_from("s", HookEvent::SessionStarted);
    assert_eq!(
        (session.status(), session.unseen()),
        (SessionStatus::Finished, true)
    );
}

const NEEDS_ACTION: SessionStatus = SessionStatus::NeedsYou {
    reason: WaitReason::Action,
};

#[test]
fn a_display_that_says_it_waits_needs_the_person_until_it_stops() {
    let mut session = Session::fresh();
    session.send(Signal::Progress { busy: true });
    assert_eq!(
        session.send(Signal::Waiting { waiting: true }),
        [status_effect(NEEDS_ACTION, true)]
    );
    // Said again, it is the same wait.
    assert!(session.send(Signal::Waiting { waiting: true }).is_empty());
    assert_eq!(
        session.send(Signal::Waiting { waiting: false }),
        [status_effect(SessionStatus::Running, false)]
    );
    session.send(Signal::Progress { busy: false });
    assert_eq!(session.status(), SessionStatus::Finished);
}

#[test]
fn a_display_waiting_closes_with_the_turn_and_keeps_a_hooked_prompt() {
    let mut session = Session::running_turn();
    session.send(Signal::Waiting { waiting: true });
    session.hook(permission(Origin::Main));
    assert_eq!(session.status(), NEEDS_PERMISSION);
    // The display stops waiting: the permission asked through a hook stays.
    session.send(Signal::Waiting { waiting: false });
    assert_eq!(session.status(), NEEDS_PERMISSION);
    // A turn's end closes the main agent's prompts.
    session.send(Signal::Waiting { waiting: true });
    session.hook(HookEvent::TurnEnded);
    assert_eq!(session.status(), SessionStatus::Finished);
}

#[test]
fn a_display_that_appears_has_started() {
    let mut session = reporting(Start::Idle);
    session.at(1);
    session.send(Signal::Waiting { waiting: false });
    assert_eq!(session.machine.next_deadline(), None);
    session.now = session.start + Timing::default().start_grace;
    assert!(session.send(Signal::Tick).is_empty());
    assert_eq!(session.status(), SessionStatus::Fresh);
}

// Waits the person closes without an answer the harness reports
mod closing;

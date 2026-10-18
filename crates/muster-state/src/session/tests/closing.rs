//! Waits that close although no tool event answers them: the person cut
//! the turn short, which Claude Code says only in its own record of the
//! conversation, never in a hook.

use super::*;

#[test]
fn declining_a_permission_ends_the_turn_and_closes_the_prompt() {
    let mut session = Session::running_turn();
    session.hook(tool_started(Origin::Main));
    session.hook(permission(Origin::Main));
    assert_eq!(session.status(), NEEDS_PERMISSION);
    // The person declined: no tool runs, no stop hook comes.
    assert_eq!(
        session.at(5).hook(HookEvent::TurnInterrupted),
        [status_effect(SessionStatus::Finished, false)],
        "the person ended it, so it is no news"
    );
    assert_eq!(session.machine.next_deadline(), None);
}

#[test]
fn an_interrupt_closes_a_question_and_a_subagents_prompt_too() {
    let mut session = Session::running_turn();
    session.hook(HookEvent::QuestionAsked {
        origin: Origin::Main,
    });
    session.hook(subagent_started("a"));
    session.hook(permission(sub("a")));
    session.at(2).hook(HookEvent::TurnInterrupted);
    // The subagent may still be at work: held, not waiting on anyone.
    assert_eq!(session.status(), SessionStatus::Running);
    assert!(!session.unseen());
    session.at(3).hook(subagent_stopped("a"));
    session.at(3 + 180).send(Signal::Tick);
    assert_eq!(session.status(), SessionStatus::Finished);
}

#[test]
fn stopping_a_working_turn_finishes_it() {
    let mut session = Session::running_turn();
    session.hook(tool_started(Origin::Main));
    session.at(1).hook(HookEvent::TurnInterrupted);
    assert_eq!(session.status(), SessionStatus::Finished);
    assert!(!session.unseen());
}

#[test]
fn an_interrupt_once_the_turn_is_over_changes_nothing() {
    let mut session = Session::running_turn();
    session.hook(HookEvent::TurnEnded);
    assert!(session.unseen());
    assert!(session.at(1).hook(HookEvent::TurnInterrupted).is_empty());
    assert_eq!(session.status(), SessionStatus::Finished);
    assert!(session.unseen(), "the finish is still news");
    let mut fresh = Session::fresh();
    assert!(fresh.hook(HookEvent::TurnInterrupted).is_empty());
    assert_eq!(fresh.status(), SessionStatus::Fresh);
}

#[test]
fn the_idle_edge_a_prompt_brings_still_leaves_it_open_until_the_interrupt() {
    // The terminal goes idle as a prompt opens, so that edge closes
    // nothing; the person declining it does.
    let mut session = Session::running_turn();
    session.send(Signal::Progress { busy: true });
    session.hook(permission(Origin::Main));
    session.send(Signal::Progress { busy: false });
    assert_eq!(session.status(), NEEDS_PERMISSION);
    session.hook(HookEvent::TurnInterrupted);
    assert_eq!(session.status(), SessionStatus::Finished);
}

#[test]
fn an_interrupt_from_another_agent_session_is_ignored() {
    let mut session = Session::launched(Launch {
        agent_session: Some("ours".to_owned()),
        start: Start::WithPrompt,
        unseen: false,
        reports_start: false,
    });
    session.hook(permission(Origin::Main));
    assert!(
        session
            .hook_from("theirs", HookEvent::TurnInterrupted)
            .is_empty()
    );
    assert_eq!(session.status(), NEEDS_PERMISSION);
}

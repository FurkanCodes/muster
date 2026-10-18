//! Waits beyond a tool's own events: the person cutting the turn short,
//! which Claude Code says only in its own record of the conversation,
//! never in a hook; and an MCP server's form for the person, whose notice
//! can trail the answer.

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
        deferred: false,
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

// An MCP server's form for the person (an elicitation): a question, and
// its notice, which can trail the answer

fn form_asked(deferred: bool) -> HookEvent {
    HookEvent::QuestionAsked {
        origin: Origin::Main,
        deferred,
    }
}

fn form_answered() -> HookEvent {
    HookEvent::ToolFinished {
        origin: Origin::Main,
        question: true,
    }
}

#[test]
fn an_mcp_form_needs_the_person_until_they_respond() {
    let mut session = Session::running_turn();
    session.hook(tool_started(Origin::Main));
    session.at(1).hook(form_asked(false));
    assert_eq!(session.status(), NEEDS_ANSWER);
    // Its notice six seconds on is the same question.
    assert!(session.at(7).hook(form_asked(true)).is_empty());
    // An answer, a decline or Esc: the tool carries on with it.
    session.at(10).hook(form_answered());
    assert_eq!(session.status(), SessionStatus::Running);
    // A notice that trails the response asks nothing new.
    session.at(11).hook(form_asked(true));
    assert_eq!(session.status(), SessionStatus::Running);
    session.at(12).hook(HookEvent::TurnEnded);
    assert_eq!(session.status(), SessionStatus::Finished);
}

#[test]
fn a_form_s_notice_alone_asks_only_while_a_turn_runs() {
    let mut session = Session::running_turn();
    session.hook(form_asked(true));
    assert_eq!(session.status(), NEEDS_ANSWER);
    session.at(3).hook(form_answered());
    assert_eq!(session.status(), SessionStatus::Running);

    let mut finished = Session::running_turn();
    finished.hook(HookEvent::TurnEnded);
    finished.at(60).hook(form_asked(true));
    assert_eq!(finished.status(), SessionStatus::Finished);
}

#[test]
fn a_subagent_s_form_is_answered_by_its_own_response() {
    let mut session = Session::running_turn();
    session.hook(subagent_started("a"));
    session.hook(HookEvent::QuestionAsked {
        origin: sub("a"),
        deferred: false,
    });
    // Claude Code's notices name no subagent: they ask nothing new.
    session.at(6).hook(form_asked(true));
    session.at(8).hook(form_answered());
    assert_eq!(
        session.status(),
        NEEDS_ANSWER,
        "the main agent answered nothing"
    );
    session.at(8).hook(HookEvent::ToolFinished {
        origin: sub("a"),
        question: true,
    });
    assert_eq!(session.status(), SessionStatus::Running);
}

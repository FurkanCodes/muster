//! The session machine under any signals, in any order, at any times: it
//! never panics, an exit is for good, and every status it reports is the
//! one it is in.

#![expect(clippy::disallowed_methods, reason = "tests time themselves")]

use std::time::{Duration, Instant};

use muster_state::session::{
    Effect, HookEvent, Launch, Origin, SessionMachine, Signal, Start, Timing,
};
use proptest::collection::vec;
use proptest::prelude::*;

fn origin() -> impl Strategy<Value = Origin> {
    prop_oneof![
        Just(Origin::Main),
        (0u8..3).prop_map(|n| Origin::Subagent(format!("sub-{n}"))),
    ]
}

fn hook() -> impl Strategy<Value = HookEvent> {
    prop_oneof![
        Just(HookEvent::SessionStarted),
        Just(HookEvent::PromptSubmitted),
        Just(HookEvent::TurnEnded),
        Just(HookEvent::TurnInterrupted),
        (origin(), any::<bool>())
            .prop_map(|(origin, deferred)| HookEvent::PermissionRequested { origin, deferred }),
        (origin(), any::<bool>())
            .prop_map(|(origin, deferred)| HookEvent::QuestionAsked { origin, deferred }),
        origin().prop_map(|origin| HookEvent::ToolStarted { origin }),
        (origin(), any::<bool>())
            .prop_map(|(origin, question)| HookEvent::ToolFinished { origin, question }),
        Just(HookEvent::Idle),
        (0u8..3).prop_map(|n| HookEvent::SubagentStarted {
            id: format!("sub-{n}")
        }),
        (0u8..3).prop_map(|n| HookEvent::SubagentStopped {
            id: format!("sub-{n}")
        }),
    ]
}

fn signal() -> impl Strategy<Value = Signal> {
    prop_oneof![
        (proptest::option::of(Just("agent-1".to_owned())), hook()).prop_map(
            |(agent_session, event)| Signal::Hook {
                agent_session,
                event
            }
        ),
        any::<bool>().prop_map(|busy| Signal::Progress { busy }),
        any::<bool>().prop_map(|waiting| Signal::Waiting { waiting }),
        (any::<bool>(), any::<bool>())
            .prop_map(|(success, requested)| Signal::Exited { success, requested }),
        any::<bool>().prop_map(|watching| Signal::Watched { watching }),
        Just(Signal::Tick),
    ]
}

fn launch() -> impl Strategy<Value = Launch> {
    (
        proptest::option::of(Just("agent-1".to_owned())),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
    )
        .prop_map(|(agent_session, prompt, unseen, reports_start)| Launch {
            agent_session,
            start: if prompt {
                Start::WithPrompt
            } else {
                Start::Idle
            },
            unseen,
            reports_start,
        })
}

proptest! {
    #[test]
    fn any_signals_at_any_times(
        launch in launch(),
        steps in vec((signal(), 0u64..120_000), 0..60),
    ) {
        let start = Instant::now();
        let mut now = start;
        let mut machine = SessionMachine::new(launch, Timing::default(), start);
        for (signal, millis) in steps {
            now += Duration::from_millis(millis);
            let was_exited = machine.status().is_exited();
            let effects = machine.step(signal, now);
            let reported = effects.iter().rev().find_map(|effect| match effect {
                Effect::Status { status, .. } => Some(*status),
                Effect::SaveResumeId(_) => None,
            });
            if let Some(status) = reported {
                prop_assert_eq!(status, machine.status());
            }
            if was_exited {
                prop_assert!(machine.status().is_exited(), "{:?}", machine.status());
            }
        }
    }
}

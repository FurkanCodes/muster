//! A plugin's hooks (`PRD-15`, `ARCHITECTURE.md` §7a): a program the
//! daemon runs as something happens ([`super::events`]), told of it, and
//! which events it takes. A hook only hears: there is no way back from it
//! into Muster, to start, prompt or type into a session (`PRD-01`). Pure:
//! [`Hook::takes`] says whether one runs for an event.

use serde::{Deserialize, Serialize};

use super::events::{Event, EventKind};
use crate::WaitReason;

/// How long a hook may run, unless its manifest says: seconds.
pub const TIMEOUT: u64 = 10;

/// The longest a manifest may let a hook run: seconds.
pub const MOST_TIMEOUT: u64 = 60;

/// A program run as something happens.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Hook {
    /// Its id within the plugin.
    pub id: String,
    /// The events it runs for.
    pub on: Vec<EventKind>,
    /// The program and its arguments, placeholders in them.
    pub command: Vec<String>,
    /// How long it may run, in seconds, before it is stopped.
    pub timeout: u64,
    /// Which of those events it takes.
    #[serde(default)]
    pub when: When,
}

/// Which events a hook takes, beyond their kind: every one given must
/// match. A filter tests only the events that carry its value, and a
/// manifest gives none no event of the hook's carries.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct When {
    /// The projects, by name.
    #[serde(default)]
    pub project: Vec<String>,
    /// The sessions' harnesses: `claude`, `codex`.
    #[serde(default)]
    pub harness: Vec<String>,
    /// Why a session waits, for `session.needs_you`.
    #[serde(default)]
    pub reason: Vec<WaitReason>,
    /// `true`: only while the person looks away from every Muster window;
    /// `false`: only while they look at one.
    #[serde(default)]
    pub away: Option<bool>,
}

impl When {
    /// Whether it lets every event through.
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }

    /// Its filters as people read them: `project muster · harness claude
    /// · away`.
    pub fn words(&self) -> String {
        let mut words = Vec::new();
        if !self.project.is_empty() {
            words.push(format!("project {}", self.project.join(", ")));
        }
        if !self.harness.is_empty() {
            words.push(format!("harness {}", self.harness.join(", ")));
        }
        if !self.reason.is_empty() {
            let reasons: Vec<&str> = self
                .reason
                .iter()
                .map(|reason| reason_name(*reason))
                .collect();
            words.push(format!("reason {}", reasons.join(", ")));
        }
        match self.away {
            Some(true) => words.push("while you are away".to_owned()),
            Some(false) => words.push("while you look at Muster".to_owned()),
            None => {}
        }
        words.join(" · ")
    }
}

/// A wait's reason, as a manifest's `reason` writes it.
pub const fn reason_name(reason: WaitReason) -> &'static str {
    match reason {
        WaitReason::Permission => "permission",
        WaitReason::Question => "question",
        WaitReason::Startup => "startup",
        WaitReason::Action => "action",
    }
}

/// Every reason, as a manifest's `reason` takes them.
pub const REASONS: [WaitReason; 4] = [
    WaitReason::Question,
    WaitReason::Permission,
    WaitReason::Startup,
    WaitReason::Action,
];

impl Hook {
    /// Whether it runs for `event`, the person `away` from every Muster
    /// window or not.
    pub fn takes(&self, event: &Event, away: bool) -> bool {
        let kind = event.event;
        if !self.on.contains(&kind) {
            return false;
        }
        let when = &self.when;
        if !when.project.is_empty()
            && !event
                .project_name()
                .is_some_and(|name| when.project.iter().any(|project| project == name))
        {
            return false;
        }
        let session = event.session.as_ref();
        if !when.harness.is_empty()
            && kind.is_a_session_s()
            && !session.is_some_and(|session| when.harness.contains(&session.harness))
        {
            return false;
        }
        if !when.reason.is_empty()
            && kind.has_reason()
            && !session
                .and_then(|session| session.reason)
                .is_some_and(|reason| when.reason.contains(&reason))
        {
            return false;
        }
        when.away.is_none_or(|wanted| wanted == away)
    }

    /// Its events as a manifest writes them: `session.needs_you,
    /// session.finished`.
    pub fn events(&self) -> String {
        self.on
            .iter()
            .map(|kind| kind.name())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::events::{EventProject, EventSession};
    use crate::time::Timestamp;

    fn hook(on: &[EventKind], when: When) -> Hook {
        Hook {
            id: "ping".to_owned(),
            on: on.to_vec(),
            command: vec!["./ping".to_owned()],
            timeout: TIMEOUT,
            when,
        }
    }

    fn needs_you(project: &str, harness: &str, reason: WaitReason) -> Event {
        let mut event = Event::sample(EventKind::SessionNeedsYou, Timestamp::from_millis(0));
        event.project = Some(EventProject {
            id: "prj_1".to_owned(),
            name: project.to_owned(),
            root: "/src/app".to_owned(),
        });
        event.session = Some(EventSession {
            id: "ses_1".to_owned(),
            name: "Fix it".to_owned(),
            harness: harness.to_owned(),
            status: "needs_you".to_owned(),
            reason: Some(reason),
            success: None,
        });
        event
    }

    #[test]
    fn a_hook_takes_only_the_events_it_names() {
        let ping = hook(&[EventKind::SessionNeedsYou], When::default());
        assert!(ping.takes(&needs_you("app", "claude", WaitReason::Question), false));
        let finished = Event::sample(EventKind::SessionFinished, Timestamp::from_millis(0));
        assert!(!ping.takes(&finished, false));
    }

    #[test]
    fn every_filter_given_must_match() {
        let ping = hook(
            &[EventKind::SessionNeedsYou],
            When {
                project: vec!["app".to_owned()],
                harness: vec!["claude".to_owned(), "codex".to_owned()],
                reason: vec![WaitReason::Permission],
                away: None,
            },
        );
        assert!(ping.takes(&needs_you("app", "codex", WaitReason::Permission), true));
        assert!(!ping.takes(&needs_you("web", "codex", WaitReason::Permission), true));
        assert!(!ping.takes(&needs_you("app", "gemini", WaitReason::Permission), true));
        assert!(!ping.takes(&needs_you("app", "claude", WaitReason::Question), true));
    }

    #[test]
    fn a_filter_tests_only_the_events_that_carry_its_value() {
        let ping = hook(
            &[
                EventKind::SessionNeedsYou,
                EventKind::SessionFinished,
                EventKind::WorktreeCreated,
            ],
            When {
                harness: vec!["claude".to_owned()],
                reason: vec![WaitReason::Question],
                ..When::default()
            },
        );
        // A finish says no reason, a worktree no harness: both pass.
        let finished = Event::sample(EventKind::SessionFinished, Timestamp::from_millis(0));
        assert!(ping.takes(&finished, false));
        let created = Event::sample(EventKind::WorktreeCreated, Timestamp::from_millis(0));
        assert!(ping.takes(&created, false));
        assert!(!ping.takes(&needs_you("app", "claude", WaitReason::Startup), false));
    }

    #[test]
    fn away_takes_an_event_only_while_the_person_looks_away_or_only_while_they_look() {
        let event = needs_you("app", "claude", WaitReason::Question);
        let away = hook(
            &[EventKind::SessionNeedsYou],
            When {
                away: Some(true),
                ..When::default()
            },
        );
        assert!(away.takes(&event, true));
        assert!(!away.takes(&event, false));
        let looking = hook(
            &[EventKind::SessionNeedsYou],
            When {
                away: Some(false),
                ..When::default()
            },
        );
        assert!(looking.takes(&event, false));
        assert!(!looking.takes(&event, true));
        let either = hook(&[EventKind::SessionNeedsYou], When::default());
        assert!(either.takes(&event, true) && either.takes(&event, false));
    }

    #[test]
    fn filters_read_as_words() {
        let when = When {
            project: vec!["muster".to_owned()],
            harness: vec!["claude".to_owned(), "codex".to_owned()],
            reason: vec![WaitReason::Question, WaitReason::Permission],
            away: Some(true),
        };
        assert_eq!(
            when.words(),
            "project muster · harness claude, codex · reason question, permission · while you \
             are away"
        );
        assert!(When::default().is_empty() && !when.is_empty());
    }
}

//! `muster session explain`: why a session has its status, as the
//! daemon's machine for it holds and the signals it last received. Every
//! time is read against the daemon's own, which the answer carries, so
//! this command reads no clock.

use std::path::Path;

use muster_core::explain::{DeadlineKind, EXPLAIN_PROTOCOL, Explanation, MachineView};
use muster_core::protocol::{Command, Reply};
use muster_core::session::Origin;
use muster_core::time::{between, span};
use muster_core::{SessionId, Timestamp, WaitReason};

use crate::commands::{Outcome, connect, describe, shown, unexpected};

pub(crate) async fn explain_session(socket: &Path, session: &str) -> Outcome {
    let session: SessionId = session.parse()?;
    let mut connection = connect(socket).await?;
    let version = connection.welcome().version;
    if version < EXPLAIN_PROTOCOL {
        return Err(format!(
            "the daemon speaks protocol {version} and cannot explain a session, which needs \
             {EXPLAIN_PROTOCOL}; `muster reload` moves it onto the version installed now"
        )
        .into());
    }
    let reply = connection
        .command(Command::ExplainSession { session })
        .await?;
    let Reply::Explanation(explanation) = reply else {
        return Err(unexpected(&reply));
    };
    print!("{}", render(&explanation));
    Ok(())
}

/// The explanation as printed, a line for each thing it says.
pub(crate) fn render(explanation: &Explanation) -> String {
    let now = explanation.now;
    let ago = |at: Timestamp| span(between(at, now));
    let mut lines = vec![format!(
        "Session {}: {}, for {}{}.",
        explanation.session,
        describe(explanation.status),
        ago(explanation.since),
        if explanation.unseen {
            ", not seen yet"
        } else {
            ""
        }
    )];
    match &explanation.machine {
        Some(machine) => lines.extend(machine_lines(machine, now)),
        None => lines.push(
            "Its program ended with an earlier daemon, so nothing more is known of it.".to_owned(),
        ),
    }
    if explanation.signals.is_empty() {
        lines.push("No signals since this daemon started its program or took it over.".to_owned());
    } else {
        lines.push("Last signals, oldest first:".to_owned());
        for seen in &explanation.signals {
            let tool = seen
                .tool
                .as_deref()
                .map(|tool| format!(" ({})", shown(tool)))
                .unwrap_or_default();
            lines.push(format!(
                "  {:>12} ago  {}{tool}",
                ago(seen.at),
                shown(&seen.what)
            ));
        }
    }
    lines.join("\n") + "\n"
}

fn machine_lines(machine: &MachineView, now: Timestamp) -> Vec<String> {
    let ago = |at: Timestamp| span(between(at, now));
    let mut lines = Vec::new();
    if machine.waits.is_empty() {
        lines.push("Waiting on nobody.".to_owned());
    } else {
        let waits: Vec<String> = machine
            .waits
            .iter()
            .map(|wait| format!("{} for {}", whose(&wait.origin), wanted(wait.reason)))
            .collect();
        let newest = if waits.len() > 1 {
            " (the last is shown)"
        } else {
            ""
        };
        lines.push(format!("Waiting on {}{newest}.", waits.join("; ")));
    }
    if !machine.subagents.is_empty() {
        let names: Vec<String> = machine.subagents.iter().map(|name| shown(name)).collect();
        lines.push(format!("Subagents at work: {}.", names.join(", ")));
    }
    if let Some(hold) = machine.hold {
        let drained = hold.drained_at.map_or_else(String::new, |at| {
            format!(", the last of which stopped {} ago", ago(at))
        });
        lines.push(format!(
            "The main agent's turn ended {} ago; its status is held for its subagents{drained}.",
            ago(hold.since)
        ));
    }
    if machine.awaiting_start {
        lines.push("It has not said it started yet.".to_owned());
    }
    if machine.watched {
        lines.push("Someone is watching it, so nothing in it is news.".to_owned());
    }
    lines.push(match machine.next {
        Some(next) => format!(
            "Next, in {}: {}.",
            span(between(now, next.at)),
            match next.kind {
                DeadlineKind::Start =>
                    "still silent, it is taken to be asking something before it starts",
                DeadlineKind::Drain => "its turn finishes, a grace after its last subagent stopped",
                DeadlineKind::Quiet => {
                    "its subagents, silent so long, are taken to be gone, and its turn finishes"
                }
            }
        ),
        None => "No deadline: only a signal changes it.".to_owned(),
    });
    lines
}

fn whose(origin: &Origin) -> String {
    match origin {
        Origin::Main => "the main agent".to_owned(),
        Origin::Subagent(id) => format!("subagent {}", shown(id)),
    }
}

fn wanted(reason: WaitReason) -> &'static str {
    match reason {
        WaitReason::Permission => "a permission",
        WaitReason::Question => "an answer",
        WaitReason::Startup => "something asked before it starts",
        WaitReason::Action => "an action its display does not name",
    }
}

#[cfg(test)]
mod tests {
    use muster_core::SessionStatus;
    use muster_core::explain::{Deadline, HoldView, OpenWait, SignalSeen};

    use super::*;

    const NOW: i64 = 1_800_000_000_000;
    const SESSION: &str = "ses_01JABCDEFGHJKMNPQRSTVWXYZ0";

    fn at(seconds_ago: i64) -> Timestamp {
        Timestamp::from_millis(NOW - seconds_ago * 1000)
    }

    fn explanation(machine: Option<MachineView>, signals: Vec<SignalSeen>) -> Explanation {
        Explanation {
            session: SESSION.parse().unwrap(),
            status: SessionStatus::NeedsYou {
                reason: WaitReason::Permission,
            },
            since: at(125),
            unseen: true,
            now: Timestamp::from_millis(NOW),
            machine,
            signals,
        }
    }

    #[test]
    fn a_session_waiting_says_on_whom_for_what_and_what_it_heard() {
        let machine = MachineView {
            watched: false,
            waits: vec![
                OpenWait {
                    origin: Origin::Subagent("a1".to_owned()),
                    reason: WaitReason::Question,
                },
                OpenWait {
                    origin: Origin::Main,
                    reason: WaitReason::Permission,
                },
            ],
            subagents: vec!["a1".to_owned()],
            hold: None,
            awaiting_start: false,
            next: None,
        };
        let signals = vec![
            SignalSeen {
                at: at(130),
                what: "hook: prompt submitted".to_owned(),
                tool: None,
            },
            SignalSeen {
                at: at(125),
                what: "hook: permission requested".to_owned(),
                tool: Some("Bash\u{1b}]0;x\u{7}".to_owned()),
            },
        ];
        let text = render(&explanation(Some(machine), signals));
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(
            lines[0],
            format!("Session {SESSION}: needs you: permission, for 2 min, not seen yet.")
        );
        assert_eq!(
            lines[1],
            "Waiting on subagent a1 for an answer; the main agent for a permission (the last is \
             shown)."
        );
        assert_eq!(lines[2], "Subagents at work: a1.");
        assert_eq!(lines[3], "No deadline: only a signal changes it.");
        assert_eq!(lines[4], "Last signals, oldest first:");
        assert_eq!(lines[5], "         2 min ago  hook: prompt submitted");
        // What an agent named reaches no terminal with its control characters.
        assert_eq!(
            lines[6],
            "         2 min ago  hook: permission requested (Bash]0;x)"
        );
    }

    #[test]
    fn a_held_turn_says_when_it_ends() {
        let machine = MachineView {
            watched: true,
            waits: Vec::new(),
            subagents: Vec::new(),
            hold: Some(HoldView {
                since: at(300),
                drained_at: Some(at(60)),
            }),
            awaiting_start: false,
            next: Some(Deadline {
                at: Timestamp::from_millis(NOW + 120_000),
                kind: DeadlineKind::Drain,
            }),
        };
        let text = render(&explanation(Some(machine), Vec::new()));
        assert!(text.contains("Waiting on nobody.\n"), "{text}");
        assert!(
            text.contains(
                "The main agent's turn ended 5 min ago; its status is held for its subagents, \
                 the last of which stopped 1 min ago.\n"
            ),
            "{text}"
        );
        assert!(text.contains("Someone is watching it"), "{text}");
        assert!(
            text.contains(
                "Next, in 2 min: its turn finishes, a grace after its last subagent stopped.\n"
            ),
            "{text}"
        );
        assert!(
            text.ends_with("No signals since this daemon started its program or took it over.\n")
        );
    }

    #[test]
    fn a_session_the_daemon_never_ran_says_so() {
        let text = render(&explanation(None, Vec::new()));
        assert!(text.contains("ended with an earlier daemon"), "{text}");
    }
}

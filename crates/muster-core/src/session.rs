//! What a session is doing, as the person sees it.

use serde::{Deserialize, Serialize};

use crate::time::Timestamp;

/// The status of one session. `muster-state` decides it; the daemon stores and
/// broadcasts it; clients draw it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum SessionStatus {
    /// Launched and waiting for its first piece of work.
    Fresh,
    /// Working on a turn.
    Running,
    /// Blocked until the person answers.
    NeedsYou {
        /// What the agent is waiting for.
        reason: WaitReason,
    },
    /// Ended its turn and is waiting for the next prompt.
    Finished,
    /// The agent process has ended.
    Exited {
        /// Whether it exited cleanly.
        success: bool,
    },
}

impl SessionStatus {
    /// Whether the agent process has ended. Nothing changes the status after this.
    pub const fn is_exited(self) -> bool {
        matches!(self, Self::Exited { .. })
    }
}

/// Why a session needs the person.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum WaitReason {
    /// The agent asked for permission to use a tool.
    Permission,
    /// The agent asked the person a question.
    Question,
    /// The agent has not started: it is asking something first, such as
    /// whether to trust the folder or to sign in.
    Startup,
    /// The agent's own display says it waits on the person, without saying
    /// for what: a harness that reports no hooks, such as Codex, whose
    /// window title reads "Action Required" for a permission and a
    /// question alike.
    Action,
}

/// Who inside an agent session raised an event, or is waited on.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum Origin {
    /// The main agent, the one the person talks to.
    Main,
    /// A subagent, by the id its harness gave it.
    Subagent(String),
}

/// What an agent is doing now, as its hooks tell: `editing view.rs`,
/// `running cargo`, `wants to run npm`, `thinking`. Only what a glance
/// needs: a file's name, never its path; a command's program, never its
/// arguments, which can carry a secret (`PRD-06`). Shown, never stored.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Activity {
    pub text: String,
    /// When it began.
    pub since: Timestamp,
}

/// The coding agent running in front in a terminal, as the daemon found
/// it there (`ARC-28`): typed by hand, or started by whatever the person
/// uses. Shown, never stored.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct AgentNow {
    /// Its harness's id: `claude`, or the name of an agent the person added.
    pub harness: String,
    /// When it was found there.
    pub since: Timestamp,
}

/// What a session's agent has used, as the agent's own records say
/// (`PRD-12`): every token it sent and got back, cached ones included, and
/// what it cost where the agent says. Shown, never stored.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Spend {
    pub tokens: u64,
    /// In millionths of a US dollar, where the agent reports a cost.
    #[serde(default)]
    pub cost_micros: Option<u64>,
}

impl Spend {
    /// Several sessions' together, `None` for none; a cost only where every
    /// one of them reports its own, as a part would read as the whole.
    pub fn total<'a>(each: impl IntoIterator<Item = &'a Self>) -> Option<Self> {
        each.into_iter().fold(None, |total, one| {
            let total = total.unwrap_or(Self {
                tokens: 0,
                cost_micros: Some(0),
            });
            Some(Self {
                tokens: total.tokens.saturating_add(one.tokens),
                cost_micros: total
                    .cost_micros
                    .zip(one.cost_micros)
                    .map(|(all, more)| all.saturating_add(more)),
            })
        })
    }

    /// `$0.42 · 1.2M tokens`, or `1.2M tokens` where no cost is reported.
    pub fn text(&self) -> String {
        let tokens = format!("{} tokens", token_count(self.tokens));
        match self.cost_micros {
            Some(micros) => format!("{} · {tokens}", dollars(micros)),
            None => tokens,
        }
    }
}

/// A count of tokens for people: `950`, `4.2k`, `12k`, `1.2M`, `34M`.
pub fn token_count(tokens: u64) -> String {
    let short = |value: u64, unit: u64, suffix: &str| {
        if value < 10 * unit {
            let tenths = value * 10 / unit;
            format!("{}.{}{suffix}", tenths / 10, tenths % 10)
        } else {
            format!("{}{suffix}", value / unit)
        }
    };
    match tokens {
        0..1_000 => tokens.to_string(),
        1_000..1_000_000 => short(tokens, 1_000, "k"),
        1_000_000..1_000_000_000 => short(tokens, 1_000_000, "M"),
        _ => short(tokens, 1_000_000_000, "B"),
    }
}

/// Dollars for people: `$0.42`, `$12.30`, and `<$0.01` for a little.
fn dollars(micros: u64) -> String {
    let cents = micros.div_ceil(10_000);
    match micros {
        0 => "$0.00".to_owned(),
        1..10_000 => "<$0.01".to_owned(),
        _ => format!("${}.{:02}", cents / 100, cents % 100),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spend_reads_as_people_count_it() {
        let tokens = |tokens| Spend {
            tokens,
            cost_micros: None,
        };
        assert_eq!(tokens(950).text(), "950 tokens");
        assert_eq!(tokens(4_250).text(), "4.2k tokens");
        assert_eq!(tokens(12_345).text(), "12k tokens");
        assert_eq!(tokens(1_234_567).text(), "1.2M tokens");
        assert_eq!(tokens(34_000_000).text(), "34M tokens");
        assert_eq!(tokens(2_500_000_000).text(), "2.5B tokens");
        let costed = |cost_micros| Spend {
            tokens: 1_000,
            cost_micros: Some(cost_micros),
        };
        assert_eq!(costed(420_000).text(), "$0.42 · 1.0k tokens");
        assert_eq!(costed(12_300_000).text(), "$12.30 · 1.0k tokens");
        assert_eq!(costed(4_000).text(), "<$0.01 · 1.0k tokens");
        assert_eq!(costed(0).text(), "$0.00 · 1.0k tokens");
    }

    #[test]
    fn a_total_says_a_cost_only_if_every_session_reports_one() {
        let one = Spend {
            tokens: 10,
            cost_micros: Some(5),
        };
        let other = Spend {
            tokens: 20,
            cost_micros: Some(7),
        };
        let unsaid = Spend {
            tokens: 30,
            cost_micros: None,
        };
        assert_eq!(Spend::total([]), None);
        assert_eq!(
            Spend::total([&one, &other]),
            Some(Spend {
                tokens: 30,
                cost_micros: Some(12)
            })
        );
        assert_eq!(
            Spend::total([&one, &unsaid, &other]),
            Some(Spend {
                tokens: 60,
                cost_micros: None
            })
        );
    }

    #[test]
    fn serializes_with_a_state_tag() {
        let status = SessionStatus::NeedsYou {
            reason: WaitReason::Question,
        };
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, r#"{"state":"needs_you","reason":"question"}"#);
        assert_eq!(
            serde_json::from_str::<SessionStatus>(&json).unwrap(),
            status
        );
        assert_eq!(
            serde_json::to_string(&SessionStatus::Finished).unwrap(),
            r#"{"state":"finished"}"#
        );
    }
}

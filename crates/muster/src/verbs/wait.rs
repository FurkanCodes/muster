//! `muster wait`: until sessions finish their turn, need the person, or
//! end, said by the exit status so a script can go on from there.

use std::collections::HashMap;
use std::process::ExitCode;
use std::time::Duration;

use muster_client::Connection;
use muster_core::protocol::{Delta, ServerFrame};
use muster_core::{SessionId, SessionStatus};

/// What a wait stops for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum Until {
    /// The agent ended its turn and waits for the next prompt.
    Finished,
    /// The agent asks the person something.
    NeedsYou,
    /// The agent's program ended.
    Exited,
}

/// Every stop, as a wait stops by default.
pub(crate) const ANY_STOP: [Until; 3] = [Until::Finished, Until::NeedsYou, Until::Exited];

/// How a session came to rest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Rest {
    Finished,
    NeedsYou,
    Exited {
        success: bool,
    },
    /// Taken off the list while waited for.
    Gone,
}

impl Rest {
    /// The exit status that says it: 0 finished (or ended well), 3 needs
    /// the person, 4 ended badly or went.
    pub(crate) fn code(self) -> u8 {
        match self {
            Self::Finished | Self::Exited { success: true } => 0,
            Self::NeedsYou => 3,
            Self::Exited { success: false } | Self::Gone => 4,
        }
    }

    pub(crate) fn words(self) -> &'static str {
        match self {
            Self::Finished => "finished its turn",
            Self::NeedsYou => "needs you",
            Self::Exited { success: true } => "ended",
            Self::Exited { success: false } => "ended with an error",
            Self::Gone => "was taken off the list",
        }
    }
}

/// The exit status of a wait that ran out of time, as `timeout(1)` says.
pub(crate) const TIMED_OUT: u8 = 124;
/// The exit status of an agent that never began on what it was sent.
pub(crate) const STALLED: u8 = 5;

/// Where `status` rests, if it is one of `until`. A session started with
/// no prompt (fresh) waits for one, as one finished does.
pub(crate) fn rest(status: SessionStatus, until: &[Until]) -> Option<Rest> {
    let (stop, rest) = match status {
        SessionStatus::Running => return None,
        SessionStatus::Fresh | SessionStatus::Finished => (Until::Finished, Rest::Finished),
        SessionStatus::NeedsYou { .. } => (Until::NeedsYou, Rest::NeedsYou),
        SessionStatus::Exited { success } => (Until::Exited, Rest::Exited { success }),
    };
    until.contains(&stop).then_some(rest)
}

/// Whether `status` shows an agent at what it was just sent: working, or
/// already asking about it.
fn begun(status: SessionStatus) -> bool {
    matches!(
        status,
        SessionStatus::Running | SessionStatus::NeedsYou { .. } | SessionStatus::Exited { .. }
    )
}

/// How a wait goes.
#[derive(Debug)]
pub(crate) struct Waiting {
    pub(crate) until: Vec<Until>,
    /// Stop when the first comes to rest, rather than when all have.
    pub(crate) any: bool,
    pub(crate) timeout: Option<Duration>,
    /// Each session was just sent work: it must be seen at it within this
    /// long before a rest counts, else it stalled.
    pub(crate) begin_within: Option<Duration>,
}

/// What a wait came to.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Waited {
    /// Each session waited for, where it came to rest, in order.
    Rested(Vec<(SessionId, Rest)>),
    TimedOut,
    Stalled(SessionId),
}

impl Waited {
    /// The exit status that says it: for several, the worst of theirs.
    pub(crate) fn code(&self) -> ExitCode {
        ExitCode::from(match self {
            Self::Rested(rests) => rests.iter().map(|(_, rest)| rest.code()).max().unwrap_or(0),
            Self::TimedOut => TIMED_OUT,
            Self::Stalled(_) => STALLED,
        })
    }
}

/// Follows `sessions` on `connection`, subscribed already with `statuses`
/// as its snapshot said, until they rest as `waiting` asks.
pub(crate) async fn follow(
    connection: &mut Connection,
    sessions: &[SessionId],
    mut statuses: HashMap<SessionId, SessionStatus>,
    waiting: &Waiting,
) -> Result<Waited, Box<dyn std::error::Error>> {
    let began = crate::instant_now();
    let deadline = waiting.timeout.map(|timeout| began + timeout);
    let mut seen_at_work: Vec<SessionId> = Vec::new();
    let mut rested: Vec<(SessionId, Rest)> = Vec::new();
    loop {
        for &session in sessions {
            if rested.iter().any(|(done, _)| *done == session) {
                continue;
            }
            let rest = match statuses.get(&session) {
                None => Some(Rest::Gone),
                Some(&status) => {
                    if begun(status) && !seen_at_work.contains(&session) {
                        seen_at_work.push(session);
                    }
                    let counts = waiting.begin_within.is_none() || seen_at_work.contains(&session);
                    rest(status, &waiting.until).filter(|_| counts)
                }
            };
            if let Some(rest) = rest {
                rested.push((session, rest));
            }
        }
        if (waiting.any && !rested.is_empty()) || rested.len() == sessions.len() {
            return Ok(Waited::Rested(rested));
        }
        let now = crate::instant_now();
        if let Some(within) = waiting.begin_within
            && now >= began + within
            && let Some(&stalled) = sessions.iter().find(|s| !seen_at_work.contains(s))
        {
            return Ok(Waited::Stalled(stalled));
        }
        if deadline.is_some_and(|deadline| now >= deadline) {
            return Ok(Waited::TimedOut);
        }
        // Looked at again at least once a second, for the deadlines.
        let next = tokio::time::timeout(Duration::from_secs(1), connection.next_event()).await;
        match next {
            Err(_) => {}
            Ok(Ok(Some(frame))) => take(frame, &mut statuses),
            Ok(Ok(None)) => return Err("the daemon closed the connection".into()),
            Ok(Err(error)) => return Err(error.into()),
        }
    }
}

/// What `frame` says of sessions' statuses.
fn take(frame: ServerFrame, statuses: &mut HashMap<SessionId, SessionStatus>) {
    match frame {
        ServerFrame::Snapshot(snapshot) => {
            *statuses = snapshot
                .sessions
                .iter()
                .map(|session| (session.id, session.status))
                .collect();
        }
        ServerFrame::Delta(Delta::SessionStatus {
            session, status, ..
        }) => {
            statuses.insert(session, status);
        }
        ServerFrame::Delta(Delta::SessionUpserted(session)) => {
            statuses.insert(session.id, session.status);
        }
        ServerFrame::Delta(Delta::SessionRemoved { id }) => {
            statuses.remove(&id);
        }
        _ => {}
    }
}

/// `30s`, `5m`, `2h` or a number of seconds.
pub(crate) fn duration(text: &str) -> Result<Duration, String> {
    let text = text.trim();
    let (number, unit) = match text.find(|c: char| !c.is_ascii_digit()) {
        Some(at) => text.split_at(at),
        None => (text, "s"),
    };
    let number: u64 = number
        .parse()
        .map_err(|_| format!("{text:?} is not a time: write 30s, 5m or 2h"))?;
    let seconds = match unit {
        "s" => number,
        "m" => number * 60,
        "h" => number * 60 * 60,
        _ => return Err(format!("{text:?} is not a time: write 30s, 5m or 2h")),
    };
    Ok(Duration::from_secs(seconds))
}

#[cfg(test)]
mod tests {
    use super::*;
    use muster_core::WaitReason;

    const QUESTION: SessionStatus = SessionStatus::NeedsYou {
        reason: WaitReason::Question,
    };

    #[test]
    fn a_wait_stops_where_it_is_asked_to() {
        assert_eq!(rest(SessionStatus::Running, &ANY_STOP), None);
        assert_eq!(
            rest(SessionStatus::Finished, &ANY_STOP),
            Some(Rest::Finished)
        );
        assert_eq!(rest(SessionStatus::Fresh, &ANY_STOP), Some(Rest::Finished));
        assert_eq!(rest(QUESTION, &ANY_STOP), Some(Rest::NeedsYou));
        assert_eq!(rest(QUESTION, &[Until::Finished]), None);
        assert_eq!(
            rest(SessionStatus::Exited { success: false }, &[Until::Exited]),
            Some(Rest::Exited { success: false })
        );
    }

    #[test]
    fn the_exit_status_says_how_it_rested_the_worst_of_several() {
        let (one, two) = (SessionId::new(), SessionId::new());
        assert_eq!(
            Waited::Rested(vec![(one, Rest::Finished)]).code(),
            ExitCode::from(0)
        );
        assert_eq!(
            Waited::Rested(vec![(one, Rest::Finished), (two, Rest::NeedsYou)]).code(),
            ExitCode::from(3)
        );
        assert_eq!(
            Waited::Rested(vec![(one, Rest::Exited { success: false })]).code(),
            ExitCode::from(4)
        );
        assert_eq!(Waited::TimedOut.code(), ExitCode::from(124));
        assert_eq!(Waited::Stalled(one).code(), ExitCode::from(5));
    }

    #[test]
    fn statuses_follow_what_the_daemon_sends() {
        let id = SessionId::new();
        let mut statuses = HashMap::from([(id, SessionStatus::Running)]);
        take(
            ServerFrame::Delta(Delta::SessionStatus {
                session: id,
                status: SessionStatus::Finished,
                unseen: true,
                since: muster_core::Timestamp::from_millis(1),
            }),
            &mut statuses,
        );
        assert_eq!(statuses.get(&id), Some(&SessionStatus::Finished));
        take(
            ServerFrame::Delta(Delta::SessionRemoved { id }),
            &mut statuses,
        );
        assert!(statuses.is_empty());
    }

    #[test]
    fn times_are_written_as_people_write_them() {
        assert_eq!(duration("30"), Ok(Duration::from_secs(30)));
        assert_eq!(duration("45s"), Ok(Duration::from_secs(45)));
        assert_eq!(duration("5m"), Ok(Duration::from_secs(300)));
        assert_eq!(duration("2h"), Ok(Duration::from_secs(7200)));
        assert!(duration("soon").is_err());
        assert!(duration("5d").is_err());
    }
}

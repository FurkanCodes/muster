//! What newly needs the person, and how it is told (`PRODUCT.md` §6): by
//! the UI's terminal, or on a Mac by a notification of the Mac's own from
//! Muster's helper; the two say the same. Pure (`ARC-01`).

use crate::{Session, SessionStatus, WaitReason};

/// Whether `session` waits on the person: asking something, or with news
/// they have not seen. A shell never does: it is the person's own, not an
/// agent's.
pub fn needs_attention(session: &Session) -> bool {
    session.harness != "shell"
        && (matches!(session.status, SessionStatus::NeedsYou { .. }) || session.unseen)
}

/// What the person is told of `session`, in the project called `project`
/// on `branch`: `Fix the login in app (work) has a question for you`.
pub fn told(session: &Session, project: Option<&str>, branch: Option<&str>) -> String {
    let what = match session.status {
        SessionStatus::NeedsYou {
            reason: WaitReason::Permission,
        } => "needs your permission",
        SessionStatus::NeedsYou {
            reason: WaitReason::Question,
        } => "has a question for you",
        SessionStatus::NeedsYou {
            reason: WaitReason::Startup,
        } => "is asking something before it starts",
        SessionStatus::NeedsYou {
            reason: WaitReason::Action,
        } => "needs you",
        SessionStatus::Exited { success: false } => "exited with an error",
        SessionStatus::Exited { success: true } => "exited",
        _ => "finished",
    };
    let project = project
        .map(|name| format!(" in {name}"))
        .unwrap_or_default();
    let place = branch
        .map(|branch| format!(" ({branch})"))
        .unwrap_or_default();
    format!("{}{project}{place} {what}", session.title)
}

/// Whether a session that came to need the person is told at once: a
/// finish is held a moment first, as an agent that ends a turn often
/// starts another at once.
pub fn told_at_once(session: &Session) -> bool {
    session.status != SessionStatus::Finished
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SessionId, Timestamp, WorkspaceId};

    fn session(harness: &str, status: SessionStatus, unseen: bool) -> Session {
        Session {
            id: SessionId::new(),
            workspace: WorkspaceId::new(),
            harness: harness.to_owned(),
            title: "Fix the login".to_owned(),
            status,
            unseen,
            resume_id: None,
            created_at: Timestamp::from_millis(0),
            ended_at: None,
            status_since: Timestamp::from_millis(0),
            activity: None,
            spend: None,
            tidy: None,
        }
    }

    #[test]
    fn an_agent_asking_or_with_news_needs_the_person_and_a_shell_never_does() {
        let asking = SessionStatus::NeedsYou {
            reason: WaitReason::Question,
        };
        assert!(needs_attention(&session("claude", asking, false)));
        assert!(needs_attention(&session(
            "claude",
            SessionStatus::Finished,
            true
        )));
        assert!(!needs_attention(&session(
            "claude",
            SessionStatus::Running,
            false
        )));
        assert!(!needs_attention(&session("shell", asking, true)));
    }

    #[test]
    fn the_person_is_told_what_where_and_why() {
        let asking = session(
            "claude",
            SessionStatus::NeedsYou {
                reason: WaitReason::Permission,
            },
            false,
        );
        assert_eq!(
            told(&asking, Some("app"), Some("work")),
            "Fix the login in app (work) needs your permission"
        );
        assert!(told_at_once(&asking));
        let finished = session("claude", SessionStatus::Finished, true);
        assert_eq!(told(&finished, None, None), "Fix the login finished");
        assert!(!told_at_once(&finished));
    }
}

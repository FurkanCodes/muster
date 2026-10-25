//! What newly needs the person, and how it is told (`PRODUCT.md` §6): by
//! the UI's terminal, or on a Mac by a notification of the Mac's own from
//! Muster's helper; the two say the same. Pure (`ARC-01`).

use std::time::Duration;

use crate::{Session, SessionStatus, WaitReason};

/// How long a finish is held before it is told, by the UI, Muster's helper
/// and plugins' hooks alike: an agent that ends a turn often starts
/// another at once.
pub const FINISH_HOLD: Duration = Duration::from_secs(2);

/// Whether `session` waits on the person: asking something, or with news
/// they have not seen. A shell or a plugin's panel never does: it is the
/// person's own or a plugin's, not an agent's.
pub fn needs_attention(session: &Session) -> bool {
    !session.kind().stands_beside()
        && (matches!(session.status, SessionStatus::NeedsYou { .. }) || session.unseen)
}

/// What the person is told of `session`, in the project called `project`
/// on `branch`: `Fix the login in app (work) has a question for you`.
pub fn told(session: &Session, project: Option<&str>, branch: Option<&str>) -> String {
    let what = what(session);
    let project = project
        .map(|name| format!(" in {name}"))
        .unwrap_or_default();
    let place = branch
        .map(|branch| format!(" ({branch})"))
        .unwrap_or_default();
    format!("{}{project}{place} {what}", session.title)
}

/// A notification of the Mac's own: where, as its title (`app · work`);
/// the chat, under it; and what happened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    pub title: String,
    pub subtitle: String,
    pub body: String,
}

/// The notification of `session`, in the project called `project` on
/// `branch`.
pub fn notice(session: &Session, project: Option<&str>, branch: Option<&str>) -> Notice {
    let title = match (project, branch) {
        (Some(project), Some(branch)) => format!("{project} · {branch}"),
        (Some(project), None) => project.to_owned(),
        (None, Some(branch)) => branch.to_owned(),
        (None, None) => "Muster".to_owned(),
    };
    let what = what(session);
    let mut body = what.chars();
    let body = body
        .next()
        .map(|first| first.to_uppercase().chain(body).collect())
        .unwrap_or_default();
    Notice {
        title,
        subtitle: session.title.clone(),
        body,
    }
}

/// What happened to `session`, in a few words.
fn what(session: &Session) -> &'static str {
    match session.status {
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
    }
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
            workspace: Some(WorkspaceId::new()),
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
            beside: None,
            started_by: None,
            agent: None,
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

    #[test]
    fn a_notification_says_where_as_its_title_the_chat_under_it_then_what() {
        let asking = session(
            "claude",
            SessionStatus::NeedsYou {
                reason: WaitReason::Question,
            },
            false,
        );
        assert_eq!(
            notice(&asking, Some("app"), Some("muster/login")),
            Notice {
                title: "app · muster/login".to_owned(),
                subtitle: "Fix the login".to_owned(),
                body: "Has a question for you".to_owned(),
            }
        );
        assert_eq!(notice(&asking, None, None).title, "Muster");
        let failed = session("claude", SessionStatus::Exited { success: false }, true);
        assert_eq!(
            notice(&failed, Some("app"), None).body,
            "Exited with an error"
        );
    }

    #[test]
    fn each_is_told_as_a_question_a_finish_or_an_error() {
        use crate::telling::Kind;
        let asking = session(
            "claude",
            SessionStatus::NeedsYou {
                reason: WaitReason::Permission,
            },
            false,
        );
        assert_eq!(Kind::of(&asking), Kind::Question);
        for finish in [
            SessionStatus::Finished,
            SessionStatus::Exited { success: true },
        ] {
            assert_eq!(Kind::of(&session("claude", finish, true)), Kind::Finish);
        }
        let failed = session("claude", SessionStatus::Exited { success: false }, true);
        assert_eq!(Kind::of(&failed), Kind::Error);
    }
}

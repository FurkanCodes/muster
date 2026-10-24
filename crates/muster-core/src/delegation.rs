//! Agents setting agents working (`PRD-01`): an agent may start a session
//! through Muster's command line, as the person could, within the limits
//! the person sets. Every session it starts names it
//! ([`Session::started_by`]), so the person sees who set what working.

use serde::{Deserialize, Serialize};

use crate::entities::Session;
use crate::ids::SessionId;

/// The first protocol whose daemon keeps [`Session::started_by`], applies
/// [`Delegation`], takes `StartSession::started_by`,
/// `CreateWorktree::named_for` and `Command::ReadSession`, and keeps
/// [`crate::settings::Preferences::delegation`] (`ARC-09`).
pub const DELEGATION_PROTOCOL: u32 = 51;

/// Where the limits are written: `[delegation] allowed`, `at_once` and
/// `depth`.
pub const DELEGATION: &str = "delegation";

/// How far agents may set agents working.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Delegation {
    /// Whether an agent may start sessions at all (`[delegation]
    /// allowed`); on unless the person turns it off.
    pub allowed: bool,
    /// How many sessions one agent's session may have started and still
    /// running at once (`[delegation] at_once`).
    pub at_once: u8,
    /// How many generations deep: 1, an agent the person started may
    /// start agents and theirs may not; 2, theirs may too, and so on
    /// (`[delegation] depth`).
    pub depth: u8,
}

impl Default for Delegation {
    fn default() -> Self {
        Self {
            allowed: true,
            at_once: 4,
            depth: 2,
        }
    }
}

impl Delegation {
    /// Why the agent in `starter` may not start another session now, in
    /// the person's words; `None` when it may. `sessions` are all the
    /// daemon's.
    pub fn refusal(&self, sessions: &[Session], starter: SessionId) -> Option<String> {
        if !self.allowed {
            return Some(
                "agents do not set agents working: Settings › Agents start agents is off".into(),
            );
        }
        let generation = generation(sessions, starter) + 1;
        if generation > u32::from(self.depth) {
            return Some(format!(
                "this agent was started by an agent {} deep, and agents may start agents only \
                 {} deep ([delegation] depth)",
                generation - 1,
                self.depth
            ));
        }
        let running = sessions
            .iter()
            .filter(|session| session.started_by == Some(starter) && !session.status.is_exited())
            .count();
        (running >= usize::from(self.at_once)).then(|| {
            format!(
                "this agent already has {running} sessions it started running, as many as one \
                 agent may at once ([delegation] at_once)"
            )
        })
    }
}

/// How many agents stand between `session` and the person: 0 for one the
/// person started. A chain that loops, which no daemon makes, ends where it
/// would come round again.
fn generation(sessions: &[Session], session: SessionId) -> u32 {
    let mut generation = 0;
    let mut at = session;
    let mut seen = vec![session];
    while let Some(parent) = sessions
        .iter()
        .find(|candidate| candidate.id == at)
        .and_then(|candidate| candidate.started_by)
    {
        if seen.contains(&parent) {
            break;
        }
        seen.push(parent);
        generation += 1;
        at = parent;
    }
    generation
}

/// The branch for an agent started on `prompt`: its first words, as a
/// branch name (`muster/fix-the-login-redirect`), at most 40 characters of
/// them; `None` where it has no words to name it by. Without `muster/`
/// unless `prefixed`.
pub fn prompt_branch(prompt: &str, prefixed: bool) -> Option<String> {
    let words: String = prompt
        .lines()
        .find(|line| !line.trim().is_empty())?
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let mut slug = String::new();
    for word in words.split('-').filter(|word| !word.is_empty()) {
        if !slug.is_empty() && slug.len() + word.len() + 1 > 40 {
            break;
        }
        if !slug.is_empty() {
            slug.push('-');
        }
        slug.push_str(&word[..word.len().min(40)]);
    }
    if slug.is_empty() {
        return None;
    }
    let prefix = if prefixed { "muster/" } else { "" };
    Some(format!("{prefix}{slug}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::WorkspaceId;
    use crate::session::SessionStatus;
    use crate::time::Timestamp;

    fn session(started_by: Option<SessionId>, status: SessionStatus) -> Session {
        Session {
            id: SessionId::new(),
            workspace: WorkspaceId::new(),
            harness: "claude".to_owned(),
            title: "t".to_owned(),
            status,
            unseen: false,
            resume_id: None,
            created_at: Timestamp::from_millis(0),
            ended_at: None,
            status_since: Timestamp::from_millis(0),
            activity: None,
            spend: None,
            tidy: None,
            beside: None,
            started_by,
            agent: None,
        }
    }

    #[test]
    fn prd_01_an_agent_may_start_agents_by_default() {
        let person = session(None, SessionStatus::Running);
        let rules = Delegation::default();
        assert_eq!(
            rules.refusal(std::slice::from_ref(&person), person.id),
            None
        );
    }

    #[test]
    fn prd_01_turned_off_no_agent_starts_one() {
        let person = session(None, SessionStatus::Running);
        let rules = Delegation {
            allowed: false,
            ..Delegation::default()
        };
        let refusal = rules
            .refusal(std::slice::from_ref(&person), person.id)
            .unwrap();
        assert!(refusal.contains("Settings"), "{refusal}");
    }

    #[test]
    fn prd_01_agents_start_agents_only_as_deep_as_the_personays() {
        let first = session(None, SessionStatus::Running);
        let second = session(Some(first.id), SessionStatus::Running);
        let third = session(Some(second.id), SessionStatus::Running);
        let sessions = [first.clone(), second.clone(), third.clone()];
        let rules = Delegation::default();
        assert_eq!(rules.refusal(&sessions, first.id), None);
        assert_eq!(rules.refusal(&sessions, second.id), None);
        assert!(
            rules
                .refusal(&sessions, third.id)
                .unwrap()
                .contains("depth")
        );
        let shallow = Delegation { depth: 1, ..rules };
        assert!(shallow.refusal(&sessions, second.id).is_some());
    }

    #[test]
    fn prd_01_an_agent_has_only_so_many_running_at_once() {
        let parent = session(None, SessionStatus::Running);
        let mut sessions = vec![parent.clone()];
        for _ in 0..3 {
            sessions.push(session(Some(parent.id), SessionStatus::Running));
        }
        sessions.push(session(
            Some(parent.id),
            SessionStatus::Exited { success: true },
        ));
        let rules = Delegation::default();
        assert_eq!(
            rules.refusal(&sessions, parent.id),
            None,
            "an ended one counts not"
        );
        sessions.push(session(Some(parent.id), SessionStatus::Finished));
        assert!(
            rules
                .refusal(&sessions, parent.id)
                .unwrap()
                .contains("at_once")
        );
    }

    #[test]
    fn a_loop_of_starters_ends() {
        let mut first = session(None, SessionStatus::Running);
        let second = session(Some(first.id), SessionStatus::Running);
        first.started_by = Some(second.id);
        assert_eq!(generation(&[first.clone(), second], first.id), 1);
    }

    #[test]
    fn a_prompt_names_its_branch_by_its_first_words() {
        assert_eq!(
            prompt_branch("Fix the login redirect!\nThen more.", true).as_deref(),
            Some("muster/fix-the-login-redirect")
        );
        assert_eq!(
            prompt_branch("\n  Add `muster ls` --json", false).as_deref(),
            Some("add-muster-ls-json")
        );
        let long = prompt_branch(&"word ".repeat(30), false).unwrap();
        assert!(long.len() <= 40, "{long}");
        assert_eq!(prompt_branch("  ¿¡ ", true), None);
        assert_eq!(prompt_branch("", true), None);
        let one = prompt_branch(&"a".repeat(80), false).unwrap();
        assert_eq!(one.len(), 40);
    }
}

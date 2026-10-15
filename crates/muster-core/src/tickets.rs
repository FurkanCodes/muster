//! Tickets and the work on them. A ticket has a key in its tracker; an
//! agent works on it in a worktree on a branch named for it,
//! `muster/issue-<key>-<title>` (or `-2`, `-3`… for further agents), so the
//! branch says which ticket its work is for.

use serde::{Deserialize, Serialize};

/// A ticket's key in its tracker: a number for GitHub and GitLab issues
/// (`42`, shown `#42`), a project's key and a number for Jira and Linear
/// (`PROJ-123`, shown as it is). Keys come from trackers anyone can write
/// to, so only these two shapes are taken: a key is safe in a branch name,
/// a command line and a terminal.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct TicketKey(String);

/// The longest key taken, so one cannot crowd out a branch name.
const MAX_KEY: usize = 40;

impl TicketKey {
    /// `text` as a key, if it is one: digits, or a project key (a letter,
    /// then letters, digits or underscores) and digits, with a dash between.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim().trim_start_matches('#');
        if text.is_empty() || text.len() > MAX_KEY {
            return None;
        }
        if text.chars().all(|c| c.is_ascii_digit()) {
            let number: u64 = text.parse().ok()?;
            return Some(Self(number.to_string()));
        }
        let (project, number) = text.rsplit_once('-')?;
        let mut letters = project.chars();
        let starts_well = letters.next().is_some_and(|c| c.is_ascii_alphabetic());
        let project_ok = starts_well && letters.all(|c| c.is_ascii_alphanumeric() || c == '_');
        let number_ok = !number.is_empty() && number.chars().all(|c| c.is_ascii_digit());
        (project_ok && number_ok)
            .then(|| Self(format!("{}-{number}", project.to_ascii_uppercase())))
    }

    /// The key for issue `number`.
    pub fn number(number: u64) -> Self {
        Self(number.to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The issue number, for a tracker that numbers its tickets.
    pub fn as_number(&self) -> Option<u64> {
        self.0.parse().ok()
    }

    /// How the person sees it: `#42`, or `PROJ-123`.
    pub fn shown(&self) -> String {
        match self.as_number() {
            Some(number) => format!("#{number}"),
            None => self.0.clone(),
        }
    }
}

impl std::fmt::Display for TicketKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for TicketKey {
    type Error = String;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        Self::parse(&text).ok_or_else(|| format!("{text:?} is not a ticket key"))
    }
}

impl From<TicketKey> for String {
    fn from(key: TicketKey) -> Self {
        key.0
    }
}

/// The ticket a branch Muster named for one is about: `muster/issue-42`,
/// `muster/issue-42-fix-the-login`, `muster/issue-PROJ-7-fix-the-login-2`,
/// or the same without `muster/`, for a project whose branches go without
/// it.
pub fn issue_of_branch(branch: &str) -> Option<TicketKey> {
    let rest = branch
        .strip_prefix("muster/issue-")
        .or_else(|| branch.strip_prefix("issue-"))?;
    let mut parts = rest.split('-');
    let first = parts.next()?;
    if !first.is_empty() && first.chars().all(|c| c.is_ascii_digit()) {
        return TicketKey::parse(first);
    }
    // A project's key keeps its case in the branch, so `PROJ-7` is not
    // taken for the start of a title.
    if first.chars().any(|c| c.is_ascii_lowercase()) {
        return None;
    }
    TicketKey::parse(&format!("{first}-{}", parts.next()?))
}

/// The branch for work on ticket `key` titled `title`:
/// `muster/issue-42-fix-the-login-redirect`, `muster/issue-PROJ-7-fix-it`;
/// without `muster/` unless `prefixed`.
pub fn ticket_branch(key: &TicketKey, title: &str, prefixed: bool) -> String {
    let prefix = if prefixed { "muster/" } else { "" };
    let words: String = title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let mut slug = String::new();
    for word in words.split('-').filter(|word| !word.is_empty()) {
        if slug.len() + word.len() + 1 > 40 {
            break;
        }
        if !slug.is_empty() {
            slug.push('-');
        }
        slug.push_str(word);
    }
    if slug.is_empty() {
        format!("{prefix}issue-{key}")
    } else {
        format!("{prefix}issue-{key}-{slug}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(text: &str) -> TicketKey {
        TicketKey::parse(text).unwrap()
    }

    #[test]
    fn keys_are_numbers_or_a_project_s_key_and_a_number() {
        assert_eq!(key("42").as_number(), Some(42));
        assert_eq!(key("#42"), key("42"));
        assert_eq!(key("007"), key("7"));
        assert_eq!(key("PROJ-123").as_str(), "PROJ-123");
        assert_eq!(key("proj-123"), key("PROJ-123"));
        assert_eq!(key("A_B2-9").as_str(), "A_B2-9");
        for not in [
            "", "-", "PROJ-", "-12", "1PROJ-2", "PR OJ-1", "PROJ-1a", "a;b-1", "x-y-1",
        ] {
            assert_eq!(TicketKey::parse(not), None, "{not:?}");
        }
        assert_eq!(TicketKey::parse(&"A".repeat(50)), None);
        assert_eq!(key("42").shown(), "#42");
        assert_eq!(key("PROJ-123").shown(), "PROJ-123");
    }

    #[test]
    fn keys_cross_the_wire_as_text_and_only_as_keys() {
        assert_eq!(
            serde_json::to_string(&key("PROJ-1")).unwrap(),
            r#""PROJ-1""#
        );
        assert_eq!(
            serde_json::from_str::<TicketKey>(r#""42""#).unwrap(),
            key("42")
        );
        assert!(serde_json::from_str::<TicketKey>(r#""$(rm -rf)""#).is_err());
    }

    #[test]
    fn a_branch_named_for_a_ticket_names_it_and_no_other_does() {
        assert_eq!(issue_of_branch("muster/issue-42"), Some(key("42")));
        assert_eq!(
            issue_of_branch("muster/issue-42-fix-the-login"),
            Some(key("42"))
        );
        assert_eq!(
            issue_of_branch("muster/issue-42-fix-the-login-2"),
            Some(key("42"))
        );
        assert_eq!(issue_of_branch("muster/issue-4"), Some(key("4")));
        assert_eq!(issue_of_branch("muster/issue-PROJ-7"), Some(key("PROJ-7")));
        assert_eq!(
            issue_of_branch("muster/issue-PROJ-7-fix-it-2"),
            Some(key("PROJ-7"))
        );
        assert_eq!(issue_of_branch("muster/issue-x-fix"), None);
        assert_eq!(issue_of_branch("muster/issue-fix-12"), None);
        assert_eq!(issue_of_branch("muster/issue-"), None);
        assert_eq!(issue_of_branch("feature/issue-42"), None);
        assert_eq!(issue_of_branch("muster/ab12cd"), None);
        // A project whose branches go without `muster/`.
        assert_eq!(issue_of_branch("issue-42-fix-the-login"), Some(key("42")));
        assert_eq!(
            issue_of_branch("issue-PROJ-7-fix-it-2"),
            Some(key("PROJ-7"))
        );
        assert_eq!(issue_of_branch("issue-fix-12"), None);
        assert_eq!(issue_of_branch("tissue-42"), None);
        assert_eq!(issue_of_branch("detached at 1a2b3c4"), None);
    }

    #[test]
    fn a_ticket_s_branch_carries_its_key_and_a_short_title() {
        assert_eq!(
            ticket_branch(&key("42"), "Fix the login redirect!", true),
            "muster/issue-42-fix-the-login-redirect"
        );
        assert_eq!(ticket_branch(&key("7"), "🔥🔥", true), "muster/issue-7");
        assert_eq!(
            ticket_branch(&key("PROJ-7"), "Fix it", true),
            "muster/issue-PROJ-7-fix-it"
        );
        assert_eq!(
            ticket_branch(&key("42"), "Fix the login redirect!", false),
            "issue-42-fix-the-login-redirect"
        );
        assert_eq!(ticket_branch(&key("7"), "🔥🔥", false), "issue-7");
        let long = ticket_branch(&key("9"), &"word ".repeat(30), true);
        assert!(long.len() <= "muster/issue-9-".len() + 40, "{long}");
        for (key, title) in [(key("PROJ-7"), "Fix it"), (key("42"), "Fix the login")] {
            for prefixed in [true, false] {
                let branch = ticket_branch(&key, title, prefixed);
                assert_eq!(issue_of_branch(&branch), Some(key.clone()), "{branch}");
            }
        }
    }
}

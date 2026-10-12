//! Where a project's tickets and pull requests live: `[tickets]` and
//! `[pull_requests]`. Neither is needed where the git remote says it,
//! GitHub or GitLab; a project whose tickets are in Jira or Linear, or whose
//! git host is not on its usual address, says so here.
//!
//! ```toml
//! [tickets]
//! tracker = "jira"                 # github, gitlab, jira or linear
//! site = "acme.atlassian.net"      # Jira: the site, or a Data Center's address
//! project = "APP"                  # Jira: the project, or `jql = "…"`
//! mine = true                      # only those assigned to you, any tracker
//!
//! [pull_requests]
//! host = "gitlab"                  # github, gitlab or bitbucket
//! ```

use serde::Deserialize;

/// A ticket tracker Muster can read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TrackerKind {
    GitHub,
    GitLab,
    Jira,
    Linear,
}

impl TrackerKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::GitHub => "GitHub issues",
            Self::GitLab => "GitLab issues",
            Self::Jira => "Jira",
            Self::Linear => "Linear",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "github" => Some(Self::GitHub),
            "gitlab" => Some(Self::GitLab),
            "jira" => Some(Self::Jira),
            "linear" => Some(Self::Linear),
            _ => None,
        }
    }
}

/// A git host Muster can open pull requests on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HostKind {
    GitHub,
    GitLab,
    Bitbucket,
}

impl HostKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::GitHub => "GitHub",
            Self::GitLab => "GitLab",
            Self::Bitbucket => "Bitbucket",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "github" => Some(Self::GitHub),
            "gitlab" => Some(Self::GitLab),
            "bitbucket" => Some(Self::Bitbucket),
            _ => None,
        }
    }

    /// The tracker that comes with the host, if it has one: Bitbucket's
    /// was retired in 2026, its teams' tickets being in Jira.
    pub fn own_tracker(self) -> Option<TrackerKind> {
        match self {
            Self::GitHub => Some(TrackerKind::GitHub),
            Self::GitLab => Some(TrackerKind::GitLab),
            Self::Bitbucket => None,
        }
    }
}

/// Where the project's tickets are, as written; what is not written
/// follows the git remote.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tickets {
    pub tracker: Option<TrackerKind>,
    /// Jira: the site, `acme.atlassian.net`, or a Data Center's address,
    /// `https://jira.example.com`.
    pub site: Option<String>,
    /// Jira: the project whose open tickets are listed.
    pub project: Option<String>,
    /// Jira: the query that lists them, instead of `project`.
    pub jql: Option<String>,
    /// Linear: the team whose open issues are listed.
    pub team: Option<String>,
    /// Only the tickets assigned to the person signed in, in any tracker.
    pub mine: bool,
}

/// Where the project's pull requests are opened; unwritten, the git
/// remote's host.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PullRequests {
    pub host: Option<HostKind>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub(super) struct RawTickets {
    tracker: Option<String>,
    site: Option<String>,
    project: Option<String>,
    jql: Option<String>,
    team: Option<String>,
    mine: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub(super) struct RawPullRequests {
    host: Option<String>,
}

/// A project's key in Jira or a team's in Linear: a letter, then letters,
/// digits or underscores.
fn is_key(text: &str) -> bool {
    let mut chars = text.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && text.len() <= 32
}

/// A value written as text, trimmed; one with control characters, or empty,
/// is refused.
fn text(key: &str, value: Option<String>) -> Result<Option<String>, String> {
    match value.map(|value| value.trim().to_owned()) {
        Some(value) if value.is_empty() => Err(format!("{key} must not be empty")),
        Some(value) if value.contains(char::is_control) => {
            Err(format!("{key} must be one line of text"))
        }
        other => Ok(other),
    }
}

impl RawTickets {
    pub(super) fn validate(self) -> Result<Tickets, String> {
        let tracker = match self.tracker {
            None => None,
            Some(name) => Some(TrackerKind::parse(&name).ok_or_else(|| {
                format!("tickets.tracker {name:?} must be github, gitlab, jira or linear")
            })?),
        };
        let site = text("tickets.site", self.site)?;
        let project = text("tickets.project", self.project)?;
        let jql = text("tickets.jql", self.jql)?;
        let team = text("tickets.team", self.team)?;
        let jira = tracker == Some(TrackerKind::Jira);
        let linear = tracker == Some(TrackerKind::Linear);
        for (key, set, belongs) in [
            ("site", site.is_some(), jira),
            ("project", project.is_some(), jira),
            ("jql", jql.is_some(), jira),
            ("team", team.is_some(), linear),
        ] {
            if set && !belongs {
                let owner = if key == "team" { "linear" } else { "jira" };
                return Err(format!(
                    "tickets.{key} is for tracker = \"{owner}\", which this project does not set"
                ));
            }
        }
        if jira {
            if site.is_none() {
                return Err(
                    "tickets.tracker is jira: say which site with tickets.site = \"acme.atlassian.net\""
                        .to_owned(),
                );
            }
            if project.is_none() && jql.is_none() {
                return Err(
                    "tickets.tracker is jira: say which tickets with tickets.project = \"KEY\" or tickets.jql"
                        .to_owned(),
                );
            }
        }
        if let Some(project) = project.as_deref().filter(|project| !is_key(project)) {
            return Err(format!(
                "tickets.project {project:?} must be a Jira project's key, such as \"APP\""
            ));
        }
        if linear && team.is_none() {
            return Err(
                "tickets.tracker is linear: say which team with tickets.team = \"KEY\"".to_owned(),
            );
        }
        if let Some(team) = team.as_deref().filter(|team| !is_key(team)) {
            return Err(format!(
                "tickets.team {team:?} must be a Linear team's key, such as \"ENG\""
            ));
        }
        Ok(Tickets {
            tracker,
            site,
            project: project.map(|project| project.to_ascii_uppercase()),
            jql,
            team: team.map(|team| team.to_ascii_uppercase()),
            mine: self.mine.unwrap_or(false),
        })
    }
}

impl RawPullRequests {
    pub(super) fn validate(self) -> Result<PullRequests, String> {
        let host = match self.host {
            None => None,
            Some(name) => Some(HostKind::parse(&name).ok_or_else(|| {
                format!("pull_requests.host {name:?} must be github, gitlab or bitbucket")
            })?),
        };
        Ok(PullRequests { host })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tickets(toml: &str) -> Result<Tickets, String> {
        toml::from_str::<RawTickets>(toml).unwrap().validate()
    }

    #[test]
    fn jira_needs_its_site_and_which_tickets() {
        let jira = tickets("tracker = \"Jira\"\nsite = \"acme.atlassian.net\"\nproject = \"app\"")
            .unwrap();
        assert_eq!(jira.tracker, Some(TrackerKind::Jira));
        assert_eq!(jira.project.as_deref(), Some("APP"));
        let by_query = tickets("tracker = \"jira\"\nsite = \"https://jira.example.com\"\njql = \"assignee = currentUser()\"").unwrap();
        assert_eq!(by_query.jql.as_deref(), Some("assignee = currentUser()"));
        for (written, says) in [
            ("tracker = \"jira\"\nproject = \"APP\"", "tickets.site"),
            (
                "tracker = \"jira\"\nsite = \"a.atlassian.net\"",
                "tickets.project",
            ),
            (
                "tracker = \"jira\"\nsite = \"a\"\nproject = \"1APP\"",
                "project's key",
            ),
            (
                "tracker = \"jira\"\nsite = \"a\"\njql = \"\"",
                "must not be empty",
            ),
            (
                "tracker = \"jira\"\nsite = \"a\"\njql = \"x\\ny\"",
                "one line",
            ),
        ] {
            let error = tickets(written).unwrap_err();
            assert!(error.contains(says), "{written:?}: {error}");
        }
    }

    #[test]
    fn linear_needs_its_team_and_settings_belong_to_their_tracker() {
        let linear = tickets("tracker = \"linear\"\nteam = \"eng\"").unwrap();
        assert_eq!(linear.team.as_deref(), Some("ENG"));
        assert!(
            tickets("tracker = \"linear\"")
                .unwrap_err()
                .contains("tickets.team")
        );
        assert!(
            tickets("tracker = \"github\"\nteam = \"ENG\"")
                .unwrap_err()
                .contains("linear")
        );
        assert!(
            tickets("site = \"a.atlassian.net\"")
                .unwrap_err()
                .contains("jira")
        );
        assert!(
            tickets("tracker = \"trello\"")
                .unwrap_err()
                .contains("github, gitlab, jira or linear")
        );
        assert_eq!(tickets("").unwrap(), Tickets::default());
        // Any tracker's, or the remote's own, can be narrowed to yours.
        assert!(tickets("mine = true").unwrap().mine);
        assert!(
            tickets("tracker = \"linear\"\nteam = \"ENG\"\nmine = true")
                .unwrap()
                .mine
        );
        assert!(!tickets("mine = false").unwrap().mine);
    }

    #[test]
    fn a_host_is_one_of_three_and_brings_its_own_tracker_if_it_has_one() {
        let host = |toml: &str| toml::from_str::<RawPullRequests>(toml).unwrap().validate();
        assert_eq!(
            host("host = \"GitLab\"").unwrap().host,
            Some(HostKind::GitLab)
        );
        assert!(
            host("host = \"gitea\"")
                .unwrap_err()
                .contains("github, gitlab or bitbucket")
        );
        assert_eq!(HostKind::GitHub.own_tracker(), Some(TrackerKind::GitHub));
        assert_eq!(HostKind::GitLab.own_tracker(), Some(TrackerKind::GitLab));
        assert_eq!(HostKind::Bitbucket.own_tracker(), None);
    }
}

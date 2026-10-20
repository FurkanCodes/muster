//! Where a project's tickets and pull requests live: `[tickets]` and
//! `[pull_requests]`. Neither is needed where the git remote says it,
//! GitHub or GitLab; a project whose tickets are in Jira or Linear, or whose
//! git host is not on its usual address, says so here.
//!
//! ```toml
//! [tickets]
//! tracker = "jira"                 # a ticket plugin: github, gitlab, jira, linear, or yours
//! site = "acme.atlassian.net"      # Jira: the site, or a Data Center's address
//! project = "APP"                  # Jira: the project, or `jql = "…"`
//! mine = true                      # only those assigned to you, any tracker
//!
//! The keys besides `tracker` and `mine` are the plugin's, as its manifest
//! declares them (`[[tickets.settings]]`).
//!
//! [pull_requests]
//! host = "gitlab"                  # github, gitlab or bitbucket
//! ```

use std::collections::BTreeMap;
use std::fmt::Write as _;

use serde::Deserialize;

use crate::plugins::{TicketSetting, own};

/// The tracker the git host comes with, by its ticket plugin's id.
pub type TrackerId = String;

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

    /// The tracker that comes with the host, if it has one, by its ticket
    /// plugin's id: Bitbucket's was retired in 2026, its teams' tickets
    /// being in Jira.
    pub fn own_tracker(self) -> Option<&'static str> {
        match self {
            Self::GitHub => Some(own::GITHUB),
            Self::GitLab => Some(own::GITLAB),
            Self::Bitbucket => None,
        }
    }
}

/// Where the project's tickets are, as written; what is not written
/// follows the git remote.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tickets {
    /// The ticket plugin they come from, by its id: `github`, `gitlab`,
    /// `jira`, `linear`, or one the person installed.
    pub tracker: Option<TrackerId>,
    /// What `[tickets]` says for that plugin, by key: the settings its
    /// manifest declares (`[[tickets.settings]]`), Jira's `site` and
    /// `project` among Muster's own.
    pub fields: BTreeMap<String, String>,
    /// Only the tickets assigned to the person signed in, in any tracker.
    pub mine: bool,
}

impl Tickets {
    /// What `[tickets]` says for `key`, if it says.
    pub fn field(&self, key: &str) -> Option<&str> {
        self.fields.get(key).map(String::as_str)
    }

    /// Where the tickets written down are, for the person, the tracker
    /// called `name`: `Jira at acme.atlassian.net, project APP`; `None`
    /// where no tracker is written, and the git remote tells.
    pub fn written(&self, name: &str) -> Option<String> {
        self.tracker.as_ref()?;
        let mut source = name.to_owned();
        // Writing to a String cannot fail.
        if let Some(site) = self.field("site") {
            let _ = write!(source, " at {site}");
        }
        let _ = match (self.field("jql"), self.field("project"), self.field("team")) {
            (Some(jql), _, _) => write!(source, ", {jql}"),
            (None, Some(project), _) => write!(source, ", project {project}"),
            (None, None, Some(team)) => write!(source, ", team {team}"),
            (None, None, None) => Ok(()),
        };
        for (key, value) in &self.fields {
            if !matches!(key.as_str(), "site" | "jql" | "project" | "team") {
                let _ = write!(source, ", {key} {value}");
            }
        }
        Some(source)
    }
}

/// The name of the tracker `id`, as the person reads it, where Muster
/// knows it without the daemon's plugins: its own plugin's name, else the
/// id.
pub fn tracker_name(id: &str) -> String {
    own::manifest(id).map_or_else(|| id.to_owned(), |manifest| manifest.name.clone())
}

/// Checks `fields`, what a project's `[tickets]` says for the ticket
/// plugin `id` whose manifest declares `settings`: each key one it
/// declares, and each it requires said.
pub fn check_fields(
    id: &str,
    settings: &[TicketSetting],
    fields: &BTreeMap<String, String>,
) -> Result<(), String> {
    for key in fields.keys() {
        if settings.iter().any(|setting| setting.key == *key) {
            continue;
        }
        if let Some(owner) = own_owner(key).filter(|owner| *owner != id) {
            return Err(format!(
                "tickets.{key} is for tracker = \"{owner}\", which this project does not set"
            ));
        }
        let known: Vec<&str> = settings
            .iter()
            .map(|setting| setting.key.as_str())
            .collect();
        return Err(if known.is_empty() {
            format!("tickets.{key} is not a setting of the {id} tracker, which takes none")
        } else {
            format!(
                "tickets.{key} is not a setting of the {id} tracker, which takes {}",
                known.join(", ")
            )
        });
    }
    match settings
        .iter()
        .find(|setting| setting.required && !fields.contains_key(&setting.key))
    {
        Some(missing) => Err(format!(
            "tickets.tracker is {id}, which needs tickets.{} ({})",
            missing.key, missing.label
        )),
        None => Ok(()),
    }
}

/// Which of Muster's own plugins declares `key`, if one does.
fn own_owner(key: &str) -> Option<&'static str> {
    own::manifests().iter().find_map(|manifest| {
        manifest
            .tickets
            .as_ref()?
            .settings
            .iter()
            .any(|setting| setting.key == key)
            .then_some(manifest.id.as_str())
    })
}

/// Where the project's pull requests are opened; unwritten, the git
/// remote's host.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PullRequests {
    pub host: Option<HostKind>,
}

/// `[tickets]` as written: Muster's own keys, and the rest for the ticket
/// plugin named, checked once it is known.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub(super) struct RawTickets {
    tracker: Option<String>,
    mine: Option<bool>,
    #[serde(flatten)]
    fields: BTreeMap<String, toml::Value>,
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
    /// Its tracker and fields, checked: against the settings of Muster's
    /// own plugin where it names one; anyone else's are checked as the
    /// daemon finds the plugin.
    pub(super) fn validate(self) -> Result<Tickets, String> {
        let tracker = match self.tracker {
            None => None,
            Some(name) => {
                let id = name.trim().to_ascii_lowercase();
                if !crate::plugins::is_id(&id) {
                    return Err(format!(
                        "tickets.tracker {name:?} must name a ticket plugin by its id: github, \
                         gitlab, jira, linear, or one you installed"
                    ));
                }
                Some(id)
            }
        };
        let mut fields = BTreeMap::new();
        for (key, value) in self.fields {
            let named = format!("tickets.{key}");
            let toml::Value::String(value) = value else {
                return Err(format!("{named} must be text"));
            };
            if let Some(value) = text(&named, Some(value))? {
                fields.insert(key, value);
            }
        }
        match tracker.as_deref() {
            None => {
                if let Some(key) = fields.keys().next() {
                    return Err(match own_owner(key) {
                        Some(owner) => format!(
                            "tickets.{key} is for tracker = \"{owner}\", which this project \
                             does not set"
                        ),
                        None => format!(
                            "tickets.{key} is for a ticket plugin: say which with \
                             tickets.tracker"
                        ),
                    });
                }
            }
            Some(id) => {
                if let Some(tickets) = own::manifest(id).and_then(|own| own.tickets.as_ref()) {
                    check_fields(id, &tickets.settings, &fields)?;
                    own_checks(id, &mut fields)?;
                }
            }
        }
        Ok(Tickets {
            tracker,
            fields,
            mine: self.mine.unwrap_or(false),
        })
    }
}

/// What Muster's own plugins need beyond their settings' keys: Jira which
/// tickets, and keys of Jira's projects and Linear's teams, which are
/// written in capitals.
fn own_checks(id: &str, fields: &mut BTreeMap<String, String>) -> Result<(), String> {
    if id == own::JIRA && !fields.contains_key("project") && !fields.contains_key("jql") {
        return Err(
            "tickets.tracker is jira: say which tickets with tickets.project = \"KEY\" or \
             tickets.jql"
                .to_owned(),
        );
    }
    for (key, whose, example) in [
        ("project", "a Jira project's", "APP"),
        ("team", "a Linear team's", "ENG"),
    ] {
        if let Some(value) = fields.get_mut(key) {
            if !is_key(value) {
                return Err(format!(
                    "tickets.{key} {value:?} must be {whose} key, such as \"{example}\""
                ));
            }
            value.make_ascii_uppercase();
        }
    }
    Ok(())
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
        assert_eq!(jira.tracker.as_deref(), Some("jira"));
        assert_eq!(jira.field("project"), Some("APP"));
        let by_query = tickets("tracker = \"jira\"\nsite = \"https://jira.example.com\"\njql = \"assignee = currentUser()\"").unwrap();
        assert_eq!(by_query.field("jql"), Some("assignee = currentUser()"));
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
    fn where_the_written_tickets_are_reads_as_words() {
        let written = |toml: &str| {
            let tickets = tickets(toml).unwrap();
            let name = tickets
                .tracker
                .as_deref()
                .map(tracker_name)
                .unwrap_or_default();
            tickets.written(&name)
        };
        assert_eq!(written("mine = true"), None);
        assert_eq!(
            written("tracker = \"github\"").as_deref(),
            Some("GitHub issues")
        );
        assert_eq!(
            written("tracker = \"jira\"\nsite = \"acme.atlassian.net\"\nproject = \"app\"")
                .as_deref(),
            Some("Jira at acme.atlassian.net, project APP")
        );
        assert_eq!(
            written("tracker = \"jira\"\nsite = \"j.example.com\"\njql = \"labels = ui\"")
                .as_deref(),
            Some("Jira at j.example.com, labels = ui")
        );
        assert_eq!(
            written("tracker = \"linear\"\nteam = \"eng\"").as_deref(),
            Some("Linear, team ENG")
        );
    }

    #[test]
    fn linear_needs_its_team_and_settings_belong_to_their_tracker() {
        let linear = tickets("tracker = \"linear\"\nteam = \"eng\"").unwrap();
        assert_eq!(linear.field("team"), Some("ENG"));
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
            tickets("tracker = \"Trello Board\"")
                .unwrap_err()
                .contains("github, gitlab, jira, linear, or one you installed")
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
    fn anyone_s_ticket_plugin_is_named_by_its_id_with_its_own_settings() {
        let youtrack = tickets("tracker = \"youtrack\"\nboard = \"APP\"\nmine = true").unwrap();
        assert_eq!(youtrack.tracker.as_deref(), Some("youtrack"));
        assert_eq!(youtrack.field("board"), Some("APP"));
        assert_eq!(
            youtrack.written("YouTrack").as_deref(),
            Some("YouTrack, board APP")
        );
        // Checked against its manifest once the daemon finds it.
        let settings = [TicketSetting {
            key: "board".to_owned(),
            label: "Board".to_owned(),
            required: true,
        }];
        assert_eq!(
            check_fields("youtrack", &settings, &youtrack.fields),
            Ok(())
        );
        let error = check_fields("youtrack", &settings, &BTreeMap::new()).unwrap_err();
        assert!(error.contains("needs tickets.board (Board)"), "{error}");
        let stray = BTreeMap::from([("colour".to_owned(), "red".to_owned())]);
        let error = check_fields("youtrack", &settings, &stray).unwrap_err();
        assert!(
            error.contains("tickets.colour is not a setting of the youtrack tracker"),
            "{error}"
        );
        // What one of Muster's own takes is named as theirs.
        let site = BTreeMap::from([("site".to_owned(), "a".to_owned())]);
        let error = check_fields("youtrack", &settings, &site).unwrap_err();
        assert!(error.contains("for tracker = \"jira\""), "{error}");
        for (written, says) in [
            (
                "tracker = \"youtrack\"\nboard = 7",
                "tickets.board must be text",
            ),
            ("colour = \"red\"", "say which with tickets.tracker"),
            ("tracker = \"github\"\ncolour = \"red\"", "which takes none"),
        ] {
            let error = tickets(written).unwrap_err();
            assert!(error.contains(says), "{written:?}: {error}");
        }
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
        assert_eq!(HostKind::GitHub.own_tracker(), Some("github"));
        assert_eq!(HostKind::GitLab.own_tracker(), Some("gitlab"));
        assert_eq!(HostKind::Bitbucket.own_tracker(), None);
    }
}

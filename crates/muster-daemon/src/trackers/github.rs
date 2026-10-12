//! GitHub issues, through the GitHub CLI (`gh`), as Nebula does: the
//! person's own `gh` login covers authentication and GitHub Enterprise, and
//! `gh` finds the repository from the checkout's remote.

use std::path::PathBuf;

use muster_core::config::TrackerKind;
use muster_core::text::{multi_line, one_line};
use muster_core::{Issue, TicketKey};
use serde::Deserialize;

use super::{BoxFuture, IssueDetail, ServiceError, TicketSource};
use crate::github::{GitHubError, gh_with};

/// How many open issues a listing shows.
const LIST_LIMIT: &str = "100";

/// A repository's GitHub issues.
#[derive(Debug)]
pub(crate) struct GitHubIssues {
    pub(crate) root: PathBuf,
}

/// `gh`'s JSON for one issue.
#[derive(Debug, Deserialize)]
struct GhIssue {
    number: u64,
    title: String,
    url: String,
    #[serde(default)]
    labels: Vec<GhLabel>,
    #[serde(default)]
    author: Option<GhAuthor>,
    #[serde(default)]
    body: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GhLabel {
    name: String,
}

#[derive(Debug, Deserialize)]
struct GhAuthor {
    login: String,
}

impl GhIssue {
    /// The issue as Muster keeps it, its text cleaned of control
    /// characters, which could reach a terminal as escape sequences.
    fn into_detail(self) -> IssueDetail {
        IssueDetail {
            issue: Issue {
                key: TicketKey::number(self.number),
                title: one_line(&self.title),
                url: one_line(&self.url),
                labels: self
                    .labels
                    .into_iter()
                    .map(|label| one_line(&label.name))
                    .collect(),
                author: self.author.map(|author| one_line(&author.login)),
            },
            body: multi_line(&self.body.unwrap_or_default()),
            tracker: TrackerKind::GitHub,
        }
    }
}

/// The issue's number: GitHub numbers its issues.
fn number(key: &TicketKey) -> Result<String, ServiceError> {
    key.as_number()
        .map(|number| number.to_string())
        .ok_or_else(|| ServiceError::NotFound(format!("{key} is not a GitHub issue's number")))
}

fn unreadable(error: serde_json::Error) -> ServiceError {
    GitHubError::Unreadable(error.to_string()).into()
}

impl TicketSource for GitHubIssues {
    fn describe(&self) -> String {
        TrackerKind::GitHub.name().to_owned()
    }

    fn list(&self) -> BoxFuture<'_, Result<Vec<Issue>, ServiceError>> {
        Box::pin(async move {
            let json = gh_with(
                &self.root,
                &[
                    "issue",
                    "list",
                    "--state",
                    "open",
                    "--limit",
                    LIST_LIMIT,
                    "--json",
                    "number,title,url,labels,author",
                ],
                None,
            )
            .await?;
            let issues: Vec<GhIssue> = serde_json::from_str(&json).map_err(unreadable)?;
            Ok(issues
                .into_iter()
                .map(|issue| issue.into_detail().issue)
                .collect())
        })
    }

    fn view<'a>(&'a self, key: &'a TicketKey) -> BoxFuture<'a, Result<IssueDetail, ServiceError>> {
        Box::pin(async move {
            let number = number(key)?;
            let json = gh_with(
                &self.root,
                &[
                    "issue",
                    "view",
                    &number,
                    "--json",
                    "number,title,url,labels,author,body",
                ],
                None,
            )
            .await?;
            let issue: GhIssue = serde_json::from_str(&json).map_err(unreadable)?;
            Ok(issue.into_detail())
        })
    }

    fn close<'a>(&'a self, key: &'a TicketKey) -> BoxFuture<'a, Result<(), ServiceError>> {
        Box::pin(async move {
            let number = number(key)?;
            gh_with(&self.root, &["issue", "close", &number], None).await?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gh_json_is_read() {
        let json = r#"[{"number":42,"title":"Fix the login redirect","url":"https://github.com/o/r/issues/42","labels":[{"name":"bug","color":"d73a4a"}],"author":{"login":"furkan","is_bot":false}}]"#;
        let issues: Vec<GhIssue> = serde_json::from_str(json).unwrap();
        let detail = issues.into_iter().next().unwrap().into_detail();
        assert_eq!(detail.issue.key, TicketKey::number(42));
        assert_eq!(detail.issue.labels, ["bug"]);
        assert_eq!(detail.issue.author.as_deref(), Some("furkan"));
        assert_eq!(detail.body, "");
        assert_eq!(detail.tracker, TrackerKind::GitHub);
    }

    #[test]
    fn issue_text_loses_control_characters_on_the_way_in() {
        let json = "{\"number\":7,\"title\":\"Evil\\u001b]0;owned\\u0007 title\\nhere\",\"url\":\"https://x/7\",\"labels\":[{\"name\":\"b\\u001b[2Jug\"}],\"author\":{\"login\":\"m\\u009bal\"},\"body\":\"line\\r\\nnext\\u001b[31m\"}";
        let detail = serde_json::from_str::<GhIssue>(json).unwrap().into_detail();
        assert_eq!(detail.issue.title, "Evil]0;owned title here");
        assert_eq!(detail.issue.labels, ["b[2Jug"]);
        assert_eq!(detail.issue.author.as_deref(), Some("mal"));
        assert_eq!(detail.body, "line\nnext[31m");
    }

    #[test]
    fn only_a_number_names_a_github_issue() {
        assert_eq!(number(&TicketKey::number(42)).unwrap(), "42");
        assert!(matches!(
            number(&TicketKey::parse("APP-1").unwrap()),
            Err(ServiceError::NotFound(_))
        ));
    }
}

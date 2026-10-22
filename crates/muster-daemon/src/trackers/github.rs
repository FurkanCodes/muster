//! GitHub issues, through the GitHub CLI (`gh`), as Nebula does: the
//! person's own `gh` login covers authentication and GitHub Enterprise, and
//! `gh` finds the repository from the checkout's remote.

use std::path::PathBuf;

use muster_core::plugins::own;
use muster_core::text::{multi_line, one_line};
use muster_core::time::parse_rfc3339;
use muster_core::{Issue, IssueComment, TicketKey};
use serde::Deserialize;

use super::{BoxFuture, IssueDetail, ServiceError, TicketSource};
use crate::github::{GitHubError, gh_with};

/// How many open issues a listing shows.
const LIST_LIMIT: &str = "100";

/// A repository's GitHub issues.
#[derive(Debug)]
pub(crate) struct GitHubIssues {
    pub(crate) root: PathBuf,
    /// Only those assigned to the person signed in to `gh`.
    pub(crate) mine: bool,
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
    /// What was said on it, oldest first: only `view` asks for them.
    #[serde(default)]
    comments: Vec<GhComment>,
}

/// `gh`'s JSON for a comment on an issue.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhComment {
    #[serde(default)]
    author: Option<GhAuthor>,
    #[serde(default)]
    body: String,
    #[serde(default)]
    created_at: Option<String>,
    /// Hidden on GitHub as spam, off-topic or outdated: left out here too.
    #[serde(default)]
    is_minimized: bool,
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
            comments: self
                .comments
                .into_iter()
                .filter(|comment| !comment.is_minimized && !comment.body.trim().is_empty())
                .map(|comment| IssueComment {
                    author: comment.author.map(|author| one_line(&author.login)),
                    at: comment.created_at.as_deref().and_then(parse_rfc3339),
                    body: multi_line(&comment.body),
                })
                .collect(),
            noun: super::Noun::own(own::GITHUB),
        }
    }
}

/// The issue's number: GitHub numbers its issues.
fn number(key: &TicketKey) -> Result<String, ServiceError> {
    key.as_number()
        .map(|number| number.to_string())
        .ok_or_else(|| ServiceError::NotFound(format!("{key} is not a GitHub issue's number")))
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "a converter for .map_err(f), which hands it the error by value"
)]
fn unreadable(error: serde_json::Error) -> ServiceError {
    GitHubError::Unreadable(error.to_string()).into()
}

impl TicketSource for GitHubIssues {
    fn describe(&self) -> String {
        muster_core::config::tracker_name(own::GITHUB)
    }

    fn only_mine(&self) -> bool {
        self.mine
    }

    fn list(&self) -> BoxFuture<'_, Result<Vec<Issue>, ServiceError>> {
        Box::pin(async move {
            let mut args = vec![
                "issue",
                "list",
                "--state",
                "open",
                "--limit",
                LIST_LIMIT,
                "--json",
                "number,title,url,labels,author",
            ];
            if self.mine {
                args.extend(["--assignee", "@me"]);
            }
            let json = gh_with(&self.root, &args, None).await?;
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
                    "number,title,url,labels,author,body,comments",
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
        assert!(detail.comments.is_empty());
        assert_eq!(detail.noun.tracker, "GitHub");
    }

    // Shaped like gh 2.83's `issue view --json …,comments`.
    #[test]
    fn its_comments_are_read_oldest_first_hidden_ones_left_out() {
        let json = r#"{"number":42,"title":"t","url":"u","body":"b","comments":[
            {"id":"IC_1","author":{"login":"octocat"},"authorAssociation":"MEMBER","body":"Seen on **main**.\r\nToo.","createdAt":"2026-10-22T09:30:00Z","isMinimized":false,"url":"u#1"},
            {"id":"IC_2","author":{"login":"spammer"},"body":"buy now","createdAt":"2026-10-22T09:31:00Z","isMinimized":true,"minimizedReason":"SPAM"},
            {"id":"IC_3","author":null,"body":"From a deleted account\u001b[2J","createdAt":"not a time"}]}"#;
        let detail = serde_json::from_str::<GhIssue>(json).unwrap().into_detail();
        assert_eq!(detail.comments.len(), 2);
        assert_eq!(detail.comments[0].author.as_deref(), Some("octocat"));
        assert_eq!(
            detail.comments[0].at,
            Some(muster_core::Timestamp::from_millis(1_792_661_400_000))
        );
        assert_eq!(detail.comments[0].body, "Seen on **main**.\nToo.");
        assert_eq!(detail.comments[1].author, None);
        assert_eq!(detail.comments[1].at, None);
        assert_eq!(detail.comments[1].body, "From a deleted account[2J");
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

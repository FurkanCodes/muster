//! GitLab issues, through the person's `glab`: their own login, the
//! project found from the checkout's remote, gitlab.com or self-managed.
//! An issue's key is its number in the project (`iid`).

use std::path::PathBuf;

use muster_core::plugins::own;
use muster_core::text::{multi_line, one_line};
use muster_core::{Issue, TicketKey};
use serde::Deserialize;

use super::{BoxFuture, IssueDetail, ServiceError, TicketSource};
use crate::glab::{glab, parse};

/// How many open issues a listing shows.
const LIST_LIMIT: &str = "100";

/// A project's GitLab issues.
#[derive(Debug)]
pub(crate) struct GitLabIssues {
    pub(crate) root: PathBuf,
    /// Only those assigned to the person signed in to `glab`.
    pub(crate) mine: bool,
}

/// `glab`'s JSON for one issue: GitLab's API fields.
#[derive(Debug, Deserialize)]
struct GlabIssue {
    iid: u64,
    title: String,
    web_url: String,
    #[serde(default)]
    labels: Vec<String>,
    #[serde(default)]
    author: Option<GlabAuthor>,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GlabAuthor {
    username: String,
}

impl GlabIssue {
    /// The issue as Muster keeps it, its text cleaned of control characters.
    fn into_detail(self) -> IssueDetail {
        IssueDetail {
            issue: Issue {
                key: TicketKey::number(self.iid),
                title: one_line(&self.title),
                url: one_line(&self.web_url),
                labels: self.labels.iter().map(|label| one_line(label)).collect(),
                author: self.author.map(|author| one_line(&author.username)),
            },
            body: multi_line(&self.description.unwrap_or_default()),
            noun: super::Noun::own(own::GITLAB),
        }
    }
}

/// The issue's number in its project.
fn iid(key: &TicketKey) -> Result<String, ServiceError> {
    key.as_number()
        .map(|number| number.to_string())
        .ok_or_else(|| ServiceError::NotFound(format!("{key} is not a GitLab issue's number")))
}

impl TicketSource for GitLabIssues {
    fn describe(&self) -> String {
        muster_core::config::tracker_name(own::GITLAB)
    }

    fn only_mine(&self) -> bool {
        self.mine
    }

    fn list(&self) -> BoxFuture<'_, Result<Vec<Issue>, ServiceError>> {
        Box::pin(async move {
            // Open ones, newest first, are what it lists unasked.
            let mut args = vec![
                "issue",
                "list",
                "--output",
                "json",
                "--per-page",
                LIST_LIMIT,
            ];
            if self.mine {
                args.push("--assignee=@me");
            }
            let json = glab(&self.root, &args).await?;
            let issues: Vec<GlabIssue> = parse(&json)?;
            Ok(issues
                .into_iter()
                .map(|issue| issue.into_detail().issue)
                .collect())
        })
    }

    fn view<'a>(&'a self, key: &'a TicketKey) -> BoxFuture<'a, Result<IssueDetail, ServiceError>> {
        Box::pin(async move {
            let iid = iid(key)?;
            let json = glab(&self.root, &["issue", "view", &iid, "--output", "json"]).await?;
            Ok(parse::<GlabIssue>(&json)?.into_detail())
        })
    }

    fn close<'a>(&'a self, key: &'a TicketKey) -> BoxFuture<'a, Result<(), ServiceError>> {
        Box::pin(async move {
            let iid = iid(key)?;
            glab(&self.root, &["issue", "close", &iid]).await?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Shaped like glab 1.122's `issue list --output json`: GitLab's API.
    const LISTED: &str = r#"[{"id":991,"iid":12,"project_id":7,"title":"Fix the login redirect","description":"After signing in, users land on /404.","state":"opened","labels":["bug","p1"],"author":{"id":3,"username":"someone","name":"Some One"},"web_url":"https://gitlab.example.com/g/app/-/issues/12","created_at":"2026-10-01T10:00:00Z"}]"#;

    #[test]
    fn glab_json_is_read() {
        let issues: Vec<GlabIssue> = parse(LISTED).unwrap();
        let detail = issues.into_iter().next().unwrap().into_detail();
        assert_eq!(detail.issue.key, TicketKey::number(12));
        assert_eq!(detail.issue.title, "Fix the login redirect");
        assert_eq!(
            detail.issue.url,
            "https://gitlab.example.com/g/app/-/issues/12"
        );
        assert_eq!(detail.issue.labels, ["bug", "p1"]);
        assert_eq!(detail.issue.author.as_deref(), Some("someone"));
        assert_eq!(detail.body, "After signing in, users land on /404.");
        assert_eq!(detail.noun.tracker, "GitLab");
    }

    #[test]
    fn issue_text_loses_control_characters_on_the_way_in() {
        let json = "{\"iid\":3,\"title\":\"Evil\\u001b]0;x\\u0007\",\"web_url\":\"https://g/3\",\"labels\":[\"a\\u001b[2Jb\"],\"description\":\"one\\u001b[31m\"}";
        let detail = parse::<GlabIssue>(json).unwrap().into_detail();
        assert_eq!(detail.issue.title, "Evil]0;x");
        assert_eq!(detail.issue.labels, ["a[2Jb"]);
        assert_eq!(detail.body, "one[31m");
        assert!(matches!(
            iid(&TicketKey::parse("APP-1").unwrap()),
            Err(ServiceError::NotFound(_))
        ));
    }
}

//! GitLab issues, through the person's `glab`: their own login, the
//! project found from the checkout's remote, gitlab.com or self-managed.
//! An issue's key is its number in the project (`iid`).

use std::path::PathBuf;

use muster_core::plugins::own;
use muster_core::text::{multi_line, one_line};
use muster_core::time::parse_rfc3339;
use muster_core::{Issue, IssueComment, TicketKey};
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

/// A note on an issue, as GitLab's API gives it: a comment, or one GitLab
/// wrote itself on a change (`system`), which is left out.
#[derive(Debug, Deserialize)]
struct GlabNote {
    #[serde(default)]
    author: Option<GlabAuthor>,
    #[serde(default)]
    body: String,
    #[serde(default)]
    created_at: Option<String>,
    #[serde(default)]
    system: bool,
}

impl GlabNote {
    /// The note as a comment, cleaned; none where GitLab wrote it or it says nothing.
    fn into_comment(self) -> Option<IssueComment> {
        (!self.system && !self.body.trim().is_empty()).then(|| IssueComment {
            author: self.author.map(|author| one_line(&author.username)),
            at: self.created_at.as_deref().and_then(parse_rfc3339),
            body: multi_line(&self.body),
        })
    }
}

/// The most notes asked for: GitLab's largest page.
const NOTES_LIMIT: &str = "100";

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
            comments: Vec::new(),
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
            // Its notes through GitLab's API, oldest first, the project
            // filled in by `glab` from the checkout's remote.
            let notes = format!(
                "projects/:fullpath/issues/{iid}/notes?sort=asc&order_by=created_at&per_page={NOTES_LIMIT}"
            );
            let viewing = ["issue", "view", &iid, "--output", "json"];
            let noting = ["api", notes.as_str()];
            let (issue, notes) =
                tokio::join!(glab(&self.root, &viewing), glab(&self.root, &noting));
            let mut detail = parse::<GlabIssue>(&issue?)?.into_detail();
            // Notes that cannot be read leave the issue as it is, without
            // them: its text, and starting work on it, never wait on them.
            match notes.and_then(|notes| parse::<Vec<GlabNote>>(&notes)) {
                Ok(notes) => {
                    detail.comments = notes
                        .into_iter()
                        .filter_map(GlabNote::into_comment)
                        .collect();
                }
                Err(error) => {
                    tracing::info!(%key, %error, "a GitLab issue's notes could not be read");
                }
            }
            Ok(detail)
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

    // Shaped like GitLab's API for an issue's notes (`GET /projects/:id/issues/:iid/notes`).
    #[test]
    fn its_notes_are_its_comments_gitlab_s_own_left_out() {
        let json = r#"[
            {"id":1,"body":"changed the description","author":{"id":3,"username":"someone"},"created_at":"2026-10-22T09:29:00.000Z","system":true},
            {"id":2,"body":"Seen on `main`\u001b[2J","author":{"id":4,"username":"other"},"created_at":"2026-10-22T11:30:00.000+02:00","system":false},
            {"id":3,"body":"  ","author":{"id":4,"username":"other"},"created_at":"2026-10-22T09:31:00Z","system":false}]"#;
        let comments: Vec<IssueComment> = parse::<Vec<GlabNote>>(json)
            .unwrap()
            .into_iter()
            .filter_map(GlabNote::into_comment)
            .collect();
        assert_eq!(
            comments,
            [IssueComment {
                author: Some("other".to_owned()),
                at: Some(muster_core::Timestamp::from_millis(1_792_661_400_000)),
                body: "Seen on `main`[2J".to_owned(),
            }]
        );
    }
}

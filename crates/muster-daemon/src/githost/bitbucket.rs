//! Bitbucket pull requests, through its REST API: Bitbucket has no command
//! line of its own to borrow a sign-in from, so the person gives Muster an
//! API token with pull request scopes (`muster auth bitbucket`, or
//! `BITBUCKET_TOKEN`), sent as a bearer token. Bitbucket Cloud
//! (`bitbucket.org`, API 2.0) and Data Center (`/rest/api/latest`, an HTTP
//! access token) are both spoken; which, and the repository, come from
//! the checkout's remote. Bitbucket's own issue tracker is gone (2026):
//! its teams' tickets are in Jira.
//!
//! Checked against Bitbucket's OpenAPI descriptions (Cloud, and Data
//! Center 10): draft pull requests (Cloud since April 2025, Data Center
//! since 8.18), finding a branch's open one, comments, and states.

use std::path::Path;

use muster_core::PullRequest;
use muster_core::remote::Remote;
use muster_core::text::one_line;
use serde::Deserialize;
use serde_json::{Value, json};

use super::{GitHost, PrState};
use crate::secrets::{Secrets, Token, token};
use crate::service::{BoxFuture, ServiceError};

/// Where Bitbucket's API is, instead of where the remote says: for tests.
const API_VARIABLE: &str = "MUSTER_BITBUCKET_API";

/// The token the person set for the daemon, over the one kept.
pub(crate) const TOKEN_VARIABLE: &str = "BITBUCKET_TOKEN";

/// The Keychain account for the token for `host`.
pub(crate) fn account(host: &str) -> String {
    format!("bitbucket:{host}")
}

/// Which Bitbucket, and the repository on it.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Repository {
    /// `bitbucket.org/<workspace>/<slug>`.
    Cloud { workspace: String, slug: String },
    /// A Data Center's project and repository.
    DataCenter { project: String, slug: String },
}

impl Repository {
    /// The repository a remote names: Cloud's `workspace/slug`; a Data
    /// Center's `scm/PROJECT/slug` over HTTPS, `project/slug` over SSH.
    fn of(remote: &Remote) -> Option<Self> {
        let parts: Vec<&str> = remote.path.split('/').collect();
        if remote.is_bitbucket_cloud() {
            return match parts[..] {
                [workspace, slug] => Some(Self::Cloud {
                    workspace: workspace.to_owned(),
                    slug: slug.to_owned(),
                }),
                _ => None,
            };
        }
        match parts[..] {
            ["scm", project, slug] | [project, slug] => Some(Self::DataCenter {
                project: project.to_owned(),
                slug: slug.to_owned(),
            }),
            _ => None,
        }
    }
}

/// Bitbucket, Cloud or a Data Center, for one repository.
#[derive(Debug)]
pub(crate) struct Bitbucket {
    http: reqwest::Client,
    /// The API's root, without a trailing `/`.
    api: String,
    host: String,
    token: Token,
    repository: Repository,
}

impl Bitbucket {
    /// Bitbucket for the repository `remote` names, with the person's token.
    pub(crate) async fn for_remote(
        remote: &Remote,
        secrets: &dyn Secrets,
        http: reqwest::Client,
    ) -> Result<Self, ServiceError> {
        let repository = Repository::of(remote).ok_or_else(|| {
            ServiceError::NotConfigured(format!(
                "{} is not the address of a Bitbucket repository Muster can read",
                remote.path
            ))
        })?;
        let api = std::env::var(API_VARIABLE)
            .ok()
            .filter(|api| !api.trim().is_empty())
            .unwrap_or_else(|| match repository {
                Repository::Cloud { .. } => "https://api.bitbucket.org/2.0".to_owned(),
                Repository::DataCenter { .. } => {
                    format!("https://{}/rest/api/latest", remote.host)
                }
            });
        let token = token(secrets, TOKEN_VARIABLE, &account(&remote.host))
            .await
            .ok_or_else(|| signed_out(&remote.host))?;
        Ok(Self {
            http,
            api: api.trim_end_matches('/').to_owned(),
            host: remote.host.clone(),
            token,
            repository,
        })
    }

    /// The repository's pull requests' root.
    fn pulls(&self) -> String {
        match &self.repository {
            Repository::Cloud { workspace, slug } => {
                format!("{}/repositories/{workspace}/{slug}/pullrequests", self.api)
            }
            Repository::DataCenter { project, slug } => {
                format!("{}/projects/{project}/repos/{slug}/pull-requests", self.api)
            }
        }
    }

    /// Sends `request` with the token, and reads the answer as JSON.
    async fn send(&self, request: reqwest::RequestBuilder) -> Result<Value, ServiceError> {
        let response = request
            .bearer_auth(self.token.value())
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|error| {
                crate::service::unreachable(&format!("Bitbucket at {}", self.host), error)
            })?;
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        if status.is_success() {
            return if text.trim().is_empty() {
                Ok(Value::Null)
            } else {
                serde_json::from_str(&text).map_err(|error| {
                    ServiceError::Unreadable(format!(
                        "Bitbucket answered something unexpected: {error}"
                    ))
                })
            };
        }
        Err(refusal(status.as_u16(), &self.host, &text))
    }

    async fn get(&self, url: &str, query: &[(&str, &str)]) -> Result<Value, ServiceError> {
        self.send(self.http.get(url).query(query)).await
    }

    async fn post(&self, url: &str, body: &Value) -> Result<Value, ServiceError> {
        self.send(self.http.post(url).json(body)).await
    }

    async fn put(&self, url: &str, body: &Value) -> Result<Value, ServiceError> {
        self.send(self.http.put(url).json(body)).await
    }

    /// The open pull request from `branch`, if there is one.
    async fn open_from(&self, branch: &str) -> Result<Option<PullRequest>, ServiceError> {
        let page = match &self.repository {
            Repository::Cloud { .. } => {
                // Drafts too: a list once left them out unless asked.
                let query = format!(
                    "source.branch.name=\"{}\" AND state=\"OPEN\" AND (draft=true OR draft=false)",
                    branch.replace('"', "")
                );
                self.get(&self.pulls(), &[("q", &query)]).await?
            }
            Repository::DataCenter { .. } => {
                let at = format!("refs/heads/{branch}");
                self.get(
                    &self.pulls(),
                    &[("at", &at), ("direction", "OUTGOING"), ("state", "OPEN")],
                )
                .await?
            }
        };
        let page: Page = serde_json::from_value(page).map_err(unreadable)?;
        Ok(page
            .values
            .into_iter()
            .next()
            .and_then(|found| found.pull_request()))
    }

    /// A Data Center pull request's version, which a change must name.
    async fn version(&self, number: u64) -> Result<u64, ServiceError> {
        let found: Pull =
            serde_json::from_value(self.get(&format!("{}/{number}", self.pulls()), &[]).await?)
                .map_err(unreadable)?;
        Ok(found.version.unwrap_or(0))
    }

    async fn set_draft(&self, number: u64, draft: bool) -> Result<(), ServiceError> {
        let url = format!("{}/{number}", self.pulls());
        let body = match &self.repository {
            Repository::Cloud { .. } => json!({ "draft": draft }),
            Repository::DataCenter { .. } => {
                json!({ "version": self.version(number).await?, "draft": draft })
            }
        };
        self.put(&url, &body).await.map(drop)
    }
}

/// What to do without a token.
fn signed_out(host: &str) -> ServiceError {
    ServiceError::NotSignedIn(format!(
        "no Bitbucket token for {host}: create an API token with pull request scopes (read and \
         write), then run `muster auth bitbucket --host {host}` (or set {TOKEN_VARIABLE} for \
         the daemon)"
    ))
}

/// What a refusal means.
fn refusal(status: u16, host: &str, text: &str) -> ServiceError {
    let said: String = one_line(text).chars().take(300).collect();
    match status {
        401 => ServiceError::NotSignedIn(format!(
            "Bitbucket did not take the token for {host} (it may have expired): make a new one and \
             run `muster auth bitbucket --host {host}`"
        )),
        403 => ServiceError::Refused(format!(
            "Bitbucket refused: the token for {host} lacks pull request scopes, or you lack access \
             ({said})"
        )),
        404 => ServiceError::NotFound(format!(
            "Bitbucket found no such repository or pull request ({said})"
        )),
        _ => ServiceError::Refused(format!("Bitbucket answered {status}: {said}")),
    }
}

fn unreadable(error: serde_json::Error) -> ServiceError {
    ServiceError::Unreadable(format!("Bitbucket answered something unexpected: {error}"))
}

/// A page of pull requests, Cloud's or a Data Center's.
#[derive(Debug, Deserialize)]
struct Page {
    #[serde(default)]
    values: Vec<Pull>,
}

/// One pull request, in either's shape.
#[derive(Debug, Deserialize)]
struct Pull {
    id: u64,
    #[serde(default)]
    state: String,
    #[serde(default)]
    links: Links,
    /// A Data Center's, for changes.
    #[serde(default)]
    version: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
struct Links {
    /// Cloud's web address.
    #[serde(default)]
    html: Option<Href>,
    /// A Data Center's web addresses.
    #[serde(default, rename = "self")]
    own: Vec<Href>,
}

#[derive(Debug, Deserialize)]
struct Href {
    href: String,
}

impl Pull {
    fn pull_request(self) -> Option<PullRequest> {
        let url = self
            .links
            .html
            .map(|href| href.href)
            .or_else(|| self.links.own.into_iter().next().map(|href| href.href))?;
        Some(PullRequest {
            number: self.id,
            url: one_line(&url),
        })
    }
}

/// Bitbucket's word for a pull request's state.
fn state(word: &str) -> PrState {
    match word {
        "MERGED" => PrState::Merged,
        "DECLINED" | "SUPERSEDED" => PrState::Closed,
        _ => PrState::Open,
    }
}

impl GitHost for Bitbucket {
    fn open_draft<'a>(
        &'a self,
        _root: &'a Path,
        branch: &'a str,
        base: &'a str,
        title: &'a str,
        body: &'a str,
    ) -> BoxFuture<'a, Result<PullRequest, ServiceError>> {
        Box::pin(async move {
            if let Some(found) = self.open_from(branch).await? {
                return Ok(found);
            }
            let request = match &self.repository {
                Repository::Cloud { .. } => json!({
                    "title": title,
                    "description": body,
                    "draft": true,
                    "source": { "branch": { "name": branch } },
                    "destination": { "branch": { "name": base } },
                }),
                Repository::DataCenter { project, slug } => {
                    let reference = |name: &str| {
                        json!({
                            "id": format!("refs/heads/{name}"),
                            "repository": { "slug": slug, "project": { "key": project } },
                        })
                    };
                    json!({
                        "title": title,
                        "description": body,
                        "draft": true,
                        "fromRef": reference(branch),
                        "toRef": reference(base),
                    })
                }
            };
            let created: Pull = serde_json::from_value(self.post(&self.pulls(), &request).await?)
                .map_err(unreadable)?;
            created.pull_request().ok_or_else(|| {
                ServiceError::Unreadable(
                    "Bitbucket opened a pull request with no address".to_owned(),
                )
            })
        })
    }

    fn mark_ready<'a>(
        &'a self,
        _root: &'a Path,
        number: u64,
    ) -> BoxFuture<'a, Result<(), ServiceError>> {
        Box::pin(self.set_draft(number, false))
    }

    fn mark_draft<'a>(
        &'a self,
        _root: &'a Path,
        number: u64,
    ) -> BoxFuture<'a, Result<(), ServiceError>> {
        Box::pin(self.set_draft(number, true))
    }

    fn review_comment<'a>(
        &'a self,
        _root: &'a Path,
        number: u64,
        body: &'a str,
    ) -> BoxFuture<'a, Result<(), ServiceError>> {
        Box::pin(async move {
            let comment = match &self.repository {
                Repository::Cloud { .. } => json!({ "content": { "raw": body } }),
                Repository::DataCenter { .. } => json!({ "text": body }),
            };
            self.post(&format!("{}/{number}/comments", self.pulls()), &comment)
                .await
                .map(drop)
        })
    }

    fn state<'a>(
        &'a self,
        _root: &'a Path,
        number: u64,
    ) -> BoxFuture<'a, Result<PrState, ServiceError>> {
        Box::pin(async move {
            let found: Pull =
                serde_json::from_value(self.get(&format!("{}/{number}", self.pulls()), &[]).await?)
                    .map_err(unreadable)?;
            Ok(state(&found.state))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repository(url: &str) -> Option<Repository> {
        Repository::of(&Remote::parse(url).unwrap())
    }

    #[test]
    fn the_repository_is_read_from_cloud_and_data_center_remotes() {
        let cloud = Some(Repository::Cloud {
            workspace: "ws".to_owned(),
            slug: "app".to_owned(),
        });
        assert_eq!(repository("git@bitbucket.org:ws/app.git"), cloud);
        assert_eq!(repository("https://me@bitbucket.org/ws/app"), cloud);
        assert_eq!(repository("https://bitbucket.org/ws/app/extra"), None);
        let data_center = Some(Repository::DataCenter {
            project: "PROJ".to_owned(),
            slug: "app".to_owned(),
        });
        assert_eq!(
            repository("https://bitbucket.example.com/scm/PROJ/app.git"),
            data_center
        );
        assert_eq!(
            repository("ssh://git@bitbucket.example.com:7999/proj/app.git"),
            Some(Repository::DataCenter {
                project: "proj".to_owned(),
                slug: "app".to_owned(),
            })
        );
    }

    #[test]
    fn pull_requests_of_either_shape_are_read() {
        let cloud: Page = serde_json::from_str(
            r#"{"values":[{"id":17,"state":"OPEN","draft":true,"links":{"html":{"href":"https://bitbucket.org/ws/app/pull-requests/17"}}}],"pagelen":10}"#,
        )
        .unwrap();
        let pr = cloud
            .values
            .into_iter()
            .next()
            .unwrap()
            .pull_request()
            .unwrap();
        assert_eq!(
            (pr.number, pr.url.as_str()),
            (17, "https://bitbucket.org/ws/app/pull-requests/17")
        );
        let data_center: Pull = serde_json::from_str(
            r#"{"id":5,"version":3,"state":"OPEN","draft":true,"links":{"self":[{"href":"https://bitbucket.example.com/projects/PROJ/repos/app/pull-requests/5"}]}}"#,
        )
        .unwrap();
        assert_eq!(data_center.version, Some(3));
        assert_eq!(data_center.pull_request().unwrap().number, 5);
        assert_eq!(state("MERGED"), PrState::Merged);
        assert_eq!(state("DECLINED"), PrState::Closed);
        assert_eq!(state("SUPERSEDED"), PrState::Closed);
        assert_eq!(state("OPEN"), PrState::Open);
    }

    #[test]
    fn refusals_say_what_to_do_and_never_carry_the_token() {
        let expired = refusal(401, "bitbucket.org", "");
        assert!(matches!(expired, ServiceError::NotSignedIn(_)));
        assert!(
            expired
                .to_string()
                .contains("muster auth bitbucket --host bitbucket.org")
        );
        assert!(matches!(refusal(403, "h", "{}"), ServiceError::Refused(_)));
        assert!(matches!(refusal(404, "h", "{}"), ServiceError::NotFound(_)));
        assert!(
            signed_out("bitbucket.org")
                .to_string()
                .contains(TOKEN_VARIABLE)
        );
        let long = refusal(500, "h", &"x\u{1b}".repeat(500)).to_string();
        assert!(long.len() < 400 && !long.contains('\u{1b}'), "{long}");
    }
}

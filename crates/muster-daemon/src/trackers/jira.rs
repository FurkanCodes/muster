//! Jira's tickets (work items), from the site and project or query in the
//! project's `[tickets]`. Reached two ways:
//!
//! - Atlassian's MCP server (Rovo MCP, v2) with the person's sign-in in
//!   their browser (`muster auth jira`), for Jira Cloud: the way agents'
//!   Atlassian connectors reach it.
//! - Jira's REST API with a token the person gave (`muster auth jira
//!   --token`, or `JIRA_API_TOKEN` with `JIRA_EMAIL`): Cloud's v3 with their
//!   email and an API token, or a Data Center's v2 with a personal access
//!   token. The way for Data Center, or where the MCP server is not let in.
//!
//! A ticket is closed through the transition whose status is in the Done
//! category, whatever the workflow names it.

use std::sync::Arc;

use muster_core::config::{Tickets, TrackerKind};
use muster_core::html;
use muster_core::text::{multi_line, one_line};
use muster_core::{Issue, TicketKey};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::Mutex;

use super::{BoxFuture, IssueDetail, ServiceError, TicketSource};
use crate::mcp::Mcp;
use crate::oauth;
use crate::secrets::{Secrets, Token};

/// The Keychain account of the person's Atlassian sign-in.
pub(crate) const MCP_ACCOUNT: &str = "jira:mcp";
/// Atlassian's MCP server, with every tool listed flat.
pub(crate) const MCP_URL: &str = "https://mcp.atlassian.com/v2/mcp?tools=all";
/// Where the MCP server is instead, and where the REST API is instead:
/// for tests.
const MCP_VARIABLE: &str = "MUSTER_JIRA_MCP";
const API_VARIABLE: &str = "MUSTER_JIRA_API";
/// The token and email the person set for the daemon.
pub(crate) const TOKEN_VARIABLE: &str = "JIRA_API_TOKEN";
pub(crate) const EMAIL_VARIABLE: &str = "JIRA_EMAIL";
/// How many open tickets a listing shows.
const LIST_LIMIT: u64 = 100;
/// The fields read.
const FIELDS: &[&str] = &["summary", "labels", "reporter", "description"];
/// The fields a listing reads.
const LISTED: &[&str] = &["summary", "labels", "reporter"];

/// The Keychain account of a token for the site at `host`.
pub(crate) fn token_account(host: &str) -> String {
    format!("jira-token:{host}")
}

/// A token, as kept: Cloud's with the email it belongs to, a Data
/// Center's alone.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct KeptToken {
    #[serde(default)]
    pub(crate) email: Option<String>,
    pub(crate) token: String,
}

/// A Jira site, as written: `acme.atlassian.net`, or a Data Center's
/// `https://jira.example.com`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Site {
    /// Its web address, without a trailing `/`.
    pub(crate) web: String,
    pub(crate) host: String,
}

impl Site {
    pub(crate) fn parse(written: &str) -> Option<Self> {
        let written = written.trim().trim_end_matches('/');
        let web = if written.contains("://") {
            written.to_owned()
        } else {
            format!("https://{written}")
        };
        let url = reqwest::Url::parse(&web).ok()?;
        let host = url.host_str()?.to_ascii_lowercase();
        Some(Self {
            web: web.trim_end_matches('/').to_owned(),
            host,
        })
    }

    pub(crate) fn is_cloud(&self) -> bool {
        self.host.ends_with(".atlassian.net")
    }
}

/// How Jira is reached.
#[derive(Debug)]
enum Access {
    Mcp {
        mcp: Mutex<Mcp>,
    },
    Rest {
        http: reqwest::Client,
        /// The API's root: `…/rest/api/3` or a Data Center's `…/rest/api/2`.
        api: String,
        email: Option<String>,
        token: Token,
    },
}

/// A project's Jira tickets.
#[derive(Debug)]
pub(crate) struct Jira {
    site: Site,
    jql: String,
    /// Whether `jql` asks only for those assigned to the person.
    mine: bool,
    access: Access,
}

/// The query for a project's open tickets, newest first.
fn project_query(project: &str) -> String {
    format!("project = \"{project}\" AND statusCategory != Done ORDER BY created DESC")
}

/// Only the tickets of `jql` assigned to the person signed in, its order
/// kept at the end.
fn assigned_to_me(jql: &str) -> String {
    let upper = jql.to_ascii_uppercase();
    let (filter, order) = match upper.rfind("ORDER BY") {
        Some(at) => (jql[..at].trim(), format!(" {}", jql[at..].trim())),
        None => (jql.trim(), String::new()),
    };
    if filter.is_empty() {
        format!("assignee = currentUser(){order}")
    } else {
        format!("({filter}) AND assignee = currentUser(){order}")
    }
}

/// The query `tickets` asks for: their JQL, or their project's open
/// tickets; with `mine`, those assigned to the person signed in.
fn query(tickets: &Tickets) -> Option<String> {
    let jql = match (&tickets.jql, &tickets.project) {
        (Some(jql), _) => jql.clone(),
        (None, Some(project)) => project_query(project),
        (None, None) => return None,
    };
    Some(if tickets.mine {
        assigned_to_me(&jql)
    } else {
        jql
    })
}

fn signed_out(site: &Site) -> ServiceError {
    let browser = if site.is_cloud() {
        "run `muster auth jira` to sign in in your browser, or "
    } else {
        ""
    };
    ServiceError::NotSignedIn(format!(
        "Muster is not signed in to Jira at {}: {browser}run `muster auth jira --token --site {}` \
         with an API token (or set {TOKEN_VARIABLE}{} for the daemon)",
        site.host,
        site.host,
        if site.is_cloud() {
            format!(" and {EMAIL_VARIABLE}")
        } else {
            String::new()
        }
    ))
}

impl Jira {
    /// Jira for `tickets`, signed in as the person.
    pub(crate) async fn for_settings(
        tickets: &Tickets,
        secrets: &dyn Secrets,
        http: reqwest::Client,
    ) -> Result<Arc<Self>, ServiceError> {
        let written = tickets.site.as_deref().unwrap_or_default();
        let site = Site::parse(written).ok_or_else(|| {
            ServiceError::NotConfigured(format!(
                "tickets.site {written:?} is not a Jira site's address"
            ))
        })?;
        let jql = query(tickets).ok_or_else(|| {
            ServiceError::NotConfigured(
                "say which tickets with tickets.project or tickets.jql".to_owned(),
            )
        })?;
        let access = if let Some(session) = oauth::session(secrets, MCP_ACCOUNT)
            .await
            .filter(|_| site.is_cloud())
        {
            let token = oauth::access_token(&http, secrets, MCP_ACCOUNT, session).await?;
            let url = std::env::var(MCP_VARIABLE).unwrap_or_else(|_| MCP_URL.to_owned());
            let mcp = Mcp::connect(http, &url, token, "Atlassian", "muster auth jira").await?;
            Access::Mcp {
                mcp: Mutex::new(mcp),
            }
        } else {
            let kept = rest_token(secrets, &site)
                .await
                .ok_or_else(|| signed_out(&site))?;
            if site.is_cloud() && kept.email.is_none() {
                return Err(signed_out(&site));
            }
            let api = std::env::var(API_VARIABLE).unwrap_or_else(|_| {
                let version = if site.is_cloud() { 3 } else { 2 };
                format!("{}/rest/api/{version}", site.web)
            });
            Access::Rest {
                http,
                api: api.trim_end_matches('/').to_owned(),
                email: kept.email,
                token: Token::new(kept.token),
            }
        };
        Ok(Arc::new(Self {
            site,
            jql,
            mine: tickets.mine,
            access,
        }))
    }

    fn browse(&self, key: &str) -> String {
        format!("{}/browse/{key}", self.site.web)
    }

    /// The site's cloud id, which Atlassian's tools ask for.
    async fn cloud_id(&self, mcp: &mut Mcp) -> Result<String, ServiceError> {
        let tool = mcp.tool(&["getAccessibleAtlassianResources"])?;
        let answer = mcp.call(&tool, json!({})).await?;
        let resources = unwrapped(&answer);
        let list = resources
            .as_array()
            .cloned()
            .or_else(|| {
                resources
                    .get("resources")
                    .and_then(Value::as_array)
                    .cloned()
            })
            .unwrap_or_default();
        list.iter()
            .find(|resource| {
                resource
                    .get("url")
                    .and_then(Value::as_str)
                    .and_then(|url| reqwest::Url::parse(url).ok())
                    .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
                    .as_deref()
                    == Some(self.site.host.as_str())
            })
            .and_then(|resource| {
                resource
                    .get("cloudId")
                    .or_else(|| resource.get("id"))
                    .and_then(Value::as_str)
            })
            .map(str::to_owned)
            .ok_or_else(|| {
                ServiceError::NotFound(format!(
                    "your Atlassian sign-in does not reach {}",
                    self.site.host
                ))
            })
    }

    /// Sends a REST request with the token, and reads the answer as JSON.
    async fn rest(&self, request: reqwest::RequestBuilder) -> Result<Value, ServiceError> {
        let Access::Rest { email, token, .. } = &self.access else {
            return Err(ServiceError::Unavailable("not Jira's REST API".to_owned()));
        };
        let request = match email {
            Some(email) => request.basic_auth(email, Some(token.value())),
            None => request.bearer_auth(token.value()),
        };
        let response = request
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|error| {
                crate::service::unreachable(&format!("Jira at {}", self.site.host), error)
            })?;
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        let said: String = one_line(&text).chars().take(300).collect();
        match status.as_u16() {
            200..=299 if text.trim().is_empty() => Ok(Value::Null),
            200..=299 => serde_json::from_str(&text).map_err(|error| {
                ServiceError::Unreadable(format!("Jira answered something unexpected: {error}"))
            }),
            401 => Err(ServiceError::NotSignedIn(format!(
                "Jira at {} did not take the token (it may have expired): run `muster auth jira \
                 --token --site {}`",
                self.site.host, self.site.host
            ))),
            403 => Err(ServiceError::Refused(format!("Jira refused (403): {said}"))),
            404 => Err(ServiceError::NotFound(format!(
                "Jira found no such ticket ({said})"
            ))),
            code => Err(ServiceError::Refused(format!(
                "Jira answered {code}: {said}"
            ))),
        }
    }

    fn rest_url(&self, path: &str) -> String {
        match &self.access {
            Access::Rest { api, .. } => format!("{api}{path}"),
            Access::Mcp { .. } => String::new(),
        }
    }

    async fn search(&self) -> Result<Value, ServiceError> {
        match &self.access {
            Access::Mcp { mcp } => {
                let mut mcp = mcp.lock().await;
                let cloud = self.cloud_id(&mut mcp).await?;
                let tool = mcp.tool(&["searchJiraIssuesUsingJql"])?;
                let args = tool.args(&[
                    (&["cloudId"], json!(cloud)),
                    (&["jql"], json!(self.jql)),
                    (&["fields"], json!(LISTED)),
                    (&["maxResults", "limit"], json!(LIST_LIMIT)),
                    // The compact view leaves labels and the reporter out.
                    (&["view"], json!("evidence")),
                ]);
                let found = mcp.call(&tool, args).await?;
                Ok(unwrapped(&found).clone())
            }
            Access::Rest { http, api, .. } => {
                if api.ends_with("/3") {
                    let body =
                        json!({ "jql": self.jql, "fields": LISTED, "maxResults": LIST_LIMIT });
                    self.rest(http.post(self.rest_url("/search/jql")).json(&body))
                        .await
                } else {
                    let fields = LISTED.join(",");
                    let limit = LIST_LIMIT.to_string();
                    let query = [
                        ("jql", self.jql.as_str()),
                        ("fields", &fields),
                        ("maxResults", &limit),
                    ];
                    self.rest(http.get(self.rest_url("/search")).query(&query))
                        .await
                }
            }
        }
    }

    async fn issue(&self, key: &TicketKey) -> Result<Value, ServiceError> {
        match &self.access {
            Access::Mcp { mcp } => {
                let mut mcp = mcp.lock().await;
                let cloud = self.cloud_id(&mut mcp).await?;
                let tool = mcp.tool(&["getJiraIssue"])?;
                // Its `fields` names custom fields only: the view says what.
                let args = tool.args(&[
                    (&["cloudId"], json!(cloud)),
                    (&["issueIdOrKey", "issueKey", "key"], json!(key.as_str())),
                    (&["view"], json!("evidence")),
                    (&["responseContentFormat"], json!("markdown")),
                ]);
                let found = mcp.call(&tool, args).await?;
                Ok(unwrapped(&found).clone())
            }
            Access::Rest { http, .. } => {
                let fields = FIELDS.join(",");
                self.rest(
                    http.get(self.rest_url(&format!("/issue/{}", key.as_str())))
                        .query(&[("fields", fields.as_str())]),
                )
                .await
            }
        }
    }

    async fn close_issue(&self, key: &TicketKey) -> Result<(), ServiceError> {
        let not_closable = || {
            ServiceError::Refused(format!(
                "no transition of {key} leads to a Done status; close it in Jira"
            ))
        };
        match &self.access {
            Access::Mcp { mcp } => {
                let mut mcp = mcp.lock().await;
                let cloud = self.cloud_id(&mut mcp).await?;
                let list = mcp.tool(&["listJiraIssueTransitions", "getTransitionsForJiraIssue"])?;
                let args = list.args(&[
                    (&["cloudId"], json!(cloud)),
                    (&["issueIdOrKey", "issueKey", "key"], json!(key.as_str())),
                ]);
                let transitions = mcp.call(&list, args).await?;
                let id = done_transition(unwrapped(&transitions)).ok_or_else(not_closable)?;
                let go = mcp.tool(&["transitionJiraIssue"])?;
                // v2 takes the id as it is; earlier, a transition object.
                let transition: (&[&str], Value) = if go
                    .input_schema
                    .pointer("/properties/transitionId")
                    .is_some()
                {
                    (&["transitionId"], json!(id))
                } else {
                    (&["transition"], json!({ "id": id }))
                };
                let args = go.args(&[
                    (&["cloudId"], json!(cloud)),
                    (&["issueIdOrKey", "issueKey", "key"], json!(key.as_str())),
                    transition,
                ]);
                mcp.call(&go, args).await.map(drop)
            }
            Access::Rest { http, .. } => {
                let url = self.rest_url(&format!("/issue/{}/transitions", key.as_str()));
                let transitions = self.rest(http.get(&url)).await?;
                let id = done_transition(&transitions).ok_or_else(not_closable)?;
                self.rest(http.post(&url).json(&json!({ "transition": { "id": id } })))
                    .await
                    .map(drop)
            }
        }
    }
}

/// The token kept for the site, or the one the person set for the daemon.
async fn rest_token(secrets: &dyn Secrets, site: &Site) -> Option<KeptToken> {
    if let Ok(token) = std::env::var(TOKEN_VARIABLE)
        && !token.trim().is_empty()
    {
        let email = std::env::var(EMAIL_VARIABLE)
            .ok()
            .map(|email| email.trim().to_owned())
            .filter(|email| !email.is_empty());
        return Some(KeptToken {
            email,
            token: token.trim().to_owned(),
        });
    }
    let kept = secrets.get(&token_account(&site.host)).await?;
    serde_json::from_str(kept.value()).ok()
}

/// The id of the transition into a Done status.
fn done_transition(found: &Value) -> Option<String> {
    let list = found
        .get("transitions")
        .and_then(Value::as_array)
        .or_else(|| found.as_array())?;
    // One the person may take now, if the server says which they may.
    list.iter()
        .filter(|transition| transition.get("isAvailable").and_then(Value::as_bool) != Some(false))
        .find(|transition| {
            transition
                .pointer("/to/statusCategory/key")
                .and_then(Value::as_str)
                == Some("done")
        })
        .and_then(|transition| match transition.get("id")? {
            Value::String(id) => Some(id.clone()),
            Value::Number(id) => Some(id.to_string()),
            _ => None,
        })
}

/// An answer without the `data` that Atlassian's MCP tools wrap it in.
fn unwrapped(answer: &Value) -> &Value {
    answer
        .get("data")
        .filter(|data| !data.is_null())
        .unwrap_or(answer)
}

/// The text of a description: plain text; the HTML Atlassian's MCP
/// server gives, as text; or Atlassian's document format with its text
/// nodes joined, paragraphs on lines of their own.
pub(crate) fn description_text(value: &Value) -> String {
    fn walk(node: &Value, out: &mut String) {
        if let Some(text) = node.get("text").and_then(Value::as_str) {
            out.push_str(text);
        }
        if node.get("type").and_then(Value::as_str) == Some("hardBreak") {
            out.push('\n');
        }
        if let Some(children) = node.get("content").and_then(Value::as_array) {
            for child in children {
                walk(child, out);
            }
        }
        if matches!(
            node.get("type").and_then(Value::as_str),
            Some("paragraph" | "heading" | "listItem" | "codeBlock" | "blockquote")
        ) && !out.ends_with('\n')
        {
            out.push('\n');
        }
    }
    match value {
        Value::String(text) if html::looks_like_html(text) => html::to_text(text),
        Value::String(text) => text.clone(),
        Value::Null => String::new(),
        node => {
            let mut out = String::new();
            walk(node, &mut out);
            out.trim_end().to_owned()
        }
    }
}

/// A work item as Jira's search and its MCP tools give it.
fn read_issue(item: &Value, browse: impl Fn(&str) -> String) -> Option<IssueDetail> {
    let key = TicketKey::parse(item.get("key")?.as_str()?)?;
    let fields = item.get("fields").unwrap_or(item);
    let text = |name: &str| fields.get(name).and_then(Value::as_str).unwrap_or_default();
    let author = fields
        .pointer("/reporter/displayName")
        .and_then(Value::as_str)
        .map(one_line);
    let labels = fields
        .get("labels")
        .and_then(Value::as_array)
        .map(|labels| {
            labels
                .iter()
                .filter_map(Value::as_str)
                .map(one_line)
                .collect()
        })
        .unwrap_or_default();
    Some(IssueDetail {
        issue: Issue {
            url: browse(key.as_str()),
            key,
            title: one_line(text("summary")),
            labels,
            author,
        },
        body: multi_line(&description_text(
            fields.get("description").unwrap_or(&Value::Null),
        )),
        tracker: TrackerKind::Jira,
    })
}

/// The work items in a search's answer.
fn read_issues(found: &Value, browse: impl Fn(&str) -> String) -> Vec<Issue> {
    let list = found
        .get("issues")
        .and_then(Value::as_array)
        .or_else(|| found.as_array())
        .cloned()
        .unwrap_or_default();
    list.iter()
        .filter_map(|item| read_issue(item, &browse))
        .map(|detail| detail.issue)
        .collect()
}

impl TicketSource for Jira {
    fn describe(&self) -> String {
        format!("Jira ({})", self.site.host)
    }

    fn only_mine(&self) -> bool {
        self.mine
    }

    fn list(&self) -> BoxFuture<'_, Result<Vec<Issue>, ServiceError>> {
        Box::pin(async move {
            let found = self.search().await?;
            Ok(read_issues(&found, |key| self.browse(key)))
        })
    }

    fn view<'a>(&'a self, key: &'a TicketKey) -> BoxFuture<'a, Result<IssueDetail, ServiceError>> {
        Box::pin(async move {
            let found = self.issue(key).await?;
            let found = found.get("issue").unwrap_or(&found);
            read_issue(found, |key| self.browse(key)).ok_or_else(|| {
                ServiceError::Unreadable(format!("Jira's answer for {key} names no ticket"))
            })
        })
    }

    fn close<'a>(&'a self, key: &'a TicketKey) -> BoxFuture<'a, Result<(), ServiceError>> {
        Box::pin(self.close_issue(key))
    }
}

#[cfg(test)]
mod tests;

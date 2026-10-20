//! Linear's issues, from the team in the project's `[tickets]`. Reached two
//! ways:
//!
//! - Linear's MCP server with the person's sign-in in their browser
//!   (`muster auth linear`): the way agents' Linear connectors reach it.
//! - Linear's GraphQL API with a personal API key the person gave
//!   (`muster auth linear --token`, or `LINEAR_API_KEY`), sent as it is.
//!
//! Open issues are those not in a completed or canceled state; closing one
//! moves it to its team's first completed state.

use std::sync::Arc;

use muster_core::config::Tickets;
use muster_core::plugins::own;
use muster_core::text::{multi_line, one_line};
use muster_core::{Issue, TicketKey};
use serde_json::{Value, json};
use tokio::sync::Mutex;

use super::{BoxFuture, IssueDetail, ServiceError, TicketSource};
use crate::mcp::Mcp;
use crate::oauth;
use crate::secrets::{Secrets, Token};

/// The Keychain account of the person's Linear sign-in, and of their key.
pub(crate) const MCP_ACCOUNT: &str = "linear:mcp";
pub(crate) const KEY_ACCOUNT: &str = "linear-token";
/// Linear's MCP server and API, and where they are instead: for tests.
pub(crate) const MCP_URL: &str = "https://mcp.linear.app/mcp";
const API_URL: &str = "https://api.linear.app/graphql";
const MCP_VARIABLE: &str = "MUSTER_LINEAR_MCP";
const API_VARIABLE: &str = "MUSTER_LINEAR_API";
/// The key the person set for the daemon.
pub(crate) const KEY_VARIABLE: &str = "LINEAR_API_KEY";
/// How many open issues a listing shows.
const LIST_LIMIT: u64 = 100;

/// How Linear is reached.
#[derive(Debug)]
enum Access {
    Mcp {
        /// One MCP session, its calls one at a time. An async lock, held
        /// across one request's calls, each bounded by the HTTP client's
        /// 30 s, and never awaited inside a `select!`, so it cannot
        /// futurelock (`ARC-13`).
        mcp: Mutex<Mcp>,
    },
    Api {
        http: reqwest::Client,
        url: String,
        key: Token,
    },
}

/// A team's Linear issues.
#[derive(Debug)]
pub(crate) struct Linear {
    team: String,
    /// Only those assigned to the person signed in.
    mine: bool,
    access: Access,
}

const LIST: &str = "query($team: String!, $first: Int!) { issues(first: $first, \
    filter: { team: { key: { eq: $team } }, state: { type: { nin: [\"completed\", \"canceled\"] } } }) \
    { nodes { identifier title url description createdAt labels { nodes { name } } creator { name } } } }";
const LIST_MINE: &str = "query($team: String!, $first: Int!) { issues(first: $first, \
    filter: { team: { key: { eq: $team } }, assignee: { isMe: { eq: true } }, \
    state: { type: { nin: [\"completed\", \"canceled\"] } } }) \
    { nodes { identifier title url description createdAt labels { nodes { name } } creator { name } } } }";
const VIEW: &str = "query($id: String!) { issue(id: $id) { identifier title url description \
    labels { nodes { name } } creator { name } } }";
const STATES: &str = "query($id: String!) { issue(id: $id) { team { states { nodes { id name type position } } } } }";
const CLOSE: &str = "mutation($id: String!, $state: String!) { issueUpdate(id: $id, input: { stateId: $state }) { success } }";

fn signed_out() -> ServiceError {
    ServiceError::NotSignedIn(format!(
        "Muster is not signed in to Linear: run `muster auth linear` to sign in in your browser, \
         or `muster auth linear --token` with a personal API key (or set {KEY_VARIABLE} for the \
         daemon)"
    ))
}

impl Linear {
    /// Linear for `tickets`, signed in as the person.
    pub(crate) async fn for_settings(
        tickets: &Tickets,
        secrets: &dyn Secrets,
        http: reqwest::Client,
    ) -> Result<Arc<Self>, ServiceError> {
        let team = tickets.field("team").map(str::to_owned).ok_or_else(|| {
            ServiceError::NotConfigured("say which team with tickets.team".to_owned())
        })?;
        let access = if let Some(session) = oauth::session(secrets, MCP_ACCOUNT).await {
            let token = oauth::access_token(&http, secrets, MCP_ACCOUNT, session).await?;
            let url = std::env::var(MCP_VARIABLE).unwrap_or_else(|_| MCP_URL.to_owned());
            let mcp = Mcp::connect(http, &url, token, "Linear", "muster auth linear").await?;
            Access::Mcp {
                mcp: Mutex::new(mcp),
            }
        } else {
            let key = crate::secrets::token(secrets, KEY_VARIABLE, KEY_ACCOUNT)
                .await
                .ok_or_else(signed_out)?;
            Access::Api {
                http,
                url: std::env::var(API_VARIABLE).unwrap_or_else(|_| API_URL.to_owned()),
                key,
            }
        };
        Ok(Arc::new(Self {
            team,
            mine: tickets.mine,
            access,
        }))
    }

    /// Runs a GraphQL query with the key: its `data`.
    async fn graphql(&self, query: &str, variables: Value) -> Result<Value, ServiceError> {
        let Access::Api { http, url, key } = &self.access else {
            return Err(ServiceError::Unavailable("not Linear's API".to_owned()));
        };
        let response = http
            .post(url)
            .header("Authorization", key.value())
            .json(&json!({ "query": query, "variables": variables }))
            .send()
            .await
            .map_err(|error| crate::service::unreachable("Linear", error))?;
        let status = response.status().as_u16();
        let text = response.text().await.unwrap_or_default();
        let said: String = one_line(&text).chars().take(300).collect();
        let answer: Value = serde_json::from_str(&text).map_err(|_| match status {
            401 | 403 => signed_out(),
            _ => ServiceError::Refused(format!("Linear answered {status}: {said}")),
        })?;
        if let Some(error) = answer
            .get("errors")
            .and_then(Value::as_array)
            .and_then(|errors| errors.first())
        {
            let message = one_line(
                error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("an error"),
            );
            let code = error
                .pointer("/extensions/code")
                .and_then(Value::as_str)
                .unwrap_or_default();
            return Err(if code == "AUTHENTICATION_ERROR" || status == 401 {
                signed_out()
            } else if message.contains("not found") || code == "ENTITY_NOT_FOUND" {
                ServiceError::NotFound(format!("Linear: {message}"))
            } else {
                ServiceError::Refused(format!("Linear: {message}"))
            });
        }
        Ok(answer.get("data").cloned().unwrap_or(Value::Null))
    }
}

/// The text of a field that may be a string, or an object with a name.
fn named(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(text) => Some(text.clone()),
        object => object
            .get("name")
            .or_else(|| object.get("displayName"))
            .and_then(Value::as_str)
            .map(str::to_owned),
    }
}

/// Whether an issue as Linear's MCP tools give it is still open: its
/// state's type, or failing that its name.
fn is_open(item: &Value) -> bool {
    let state = item.get("state").or_else(|| item.get("status"));
    let kind = state
        .and_then(|state| state.get("type"))
        .and_then(Value::as_str)
        .map(str::to_ascii_lowercase);
    if let Some(kind) = kind {
        return !matches!(kind.as_str(), "completed" | "canceled" | "cancelled");
    }
    let name = named(state).unwrap_or_default().to_ascii_lowercase();
    !matches!(
        name.as_str(),
        "done" | "completed" | "canceled" | "cancelled" | "duplicate"
    )
}

/// An issue, as the API or the MCP tools give it.
fn read_issue(item: &Value) -> Option<IssueDetail> {
    let key = ["identifier", "id"].iter().find_map(|field| {
        item.get(*field)
            .and_then(Value::as_str)
            .and_then(TicketKey::parse)
    })?;
    let text = |field: &str| item.get(field).and_then(Value::as_str).unwrap_or_default();
    let labels = match item.get("labels") {
        Some(Value::Array(labels)) => labels
            .iter()
            .filter_map(|label| named(Some(label)))
            .collect(),
        Some(object) => object
            .get("nodes")
            .and_then(Value::as_array)
            .map(|nodes| {
                nodes
                    .iter()
                    .filter_map(|label| named(Some(label)))
                    .collect()
            })
            .unwrap_or_default(),
        None => Vec::new(),
    };
    let author = named(item.get("creator").or_else(|| item.get("createdBy")));
    Some(IssueDetail {
        issue: Issue {
            key,
            title: one_line(text("title")),
            url: one_line(text("url")),
            labels: labels.iter().map(|label| one_line(label)).collect(),
            author: author.map(|author| one_line(&author)),
        },
        body: multi_line(text("description")),
        noun: super::Noun::own(own::LINEAR),
    })
}

/// The issues in an answer: a list, or one under `issues` or `nodes`.
fn items(found: &Value) -> Vec<Value> {
    found
        .as_array()
        .or_else(|| found.get("issues").and_then(Value::as_array))
        .or_else(|| found.pointer("/issues/nodes").and_then(Value::as_array))
        .or_else(|| found.get("nodes").and_then(Value::as_array))
        .cloned()
        .unwrap_or_default()
}

/// The first completed state among `states`: its id and name.
fn completed(states: &[Value]) -> Option<(String, String)> {
    let mut done: Vec<&Value> = states
        .iter()
        .filter(|state| state.get("type").and_then(Value::as_str) == Some("completed"))
        .collect();
    done.sort_by(|one, other| {
        let position = |state: &Value| state.get("position").and_then(Value::as_f64).unwrap_or(0.0);
        position(one).total_cmp(&position(other))
    });
    let first = done.first()?;
    Some((
        first.get("id").and_then(Value::as_str)?.to_owned(),
        first
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
    ))
}

impl TicketSource for Linear {
    fn describe(&self) -> String {
        format!("Linear ({})", self.team)
    }

    fn only_mine(&self) -> bool {
        self.mine
    }

    fn source(&self) -> Option<String> {
        let whose = if self.mine { "mine" } else { "all" };
        Some(format!("linear {} {whose}", self.team))
    }

    fn list(&self) -> BoxFuture<'_, Result<Vec<Issue>, ServiceError>> {
        Box::pin(async move {
            let mut found: Vec<(String, Issue)> = match &self.access {
                Access::Api { .. } => {
                    let data = self
                        .graphql(
                            if self.mine { LIST_MINE } else { LIST },
                            json!({ "team": self.team, "first": LIST_LIMIT }),
                        )
                        .await?;
                    items(data.get("issues").unwrap_or(&Value::Null))
                        .iter()
                        .filter_map(|item| {
                            let created = item
                                .get("createdAt")
                                .and_then(Value::as_str)
                                .unwrap_or_default();
                            read_issue(item).map(|detail| (created.to_owned(), detail.issue))
                        })
                        .collect()
                }
                Access::Mcp { mcp } => {
                    let mut mcp = mcp.lock().await;
                    let tool = mcp.tool(&["list_issues", "listIssues"])?;
                    let mut wanted: Vec<(&[&str], Value)> = vec![
                        (&["team", "teamId", "teamKey"], json!(self.team)),
                        (&["limit", "first"], json!(LIST_LIMIT)),
                    ];
                    if self.mine {
                        wanted.push((&["assignee", "assigneeId"], json!("me")));
                    }
                    let args = tool.args(&wanted);
                    let answer = mcp.call(&tool, args).await?;
                    items(&answer)
                        .iter()
                        .filter(|item| is_open(item))
                        .filter_map(|item| {
                            let created = item
                                .get("createdAt")
                                .and_then(Value::as_str)
                                .unwrap_or_default();
                            read_issue(item).map(|detail| (created.to_owned(), detail.issue))
                        })
                        .collect()
                }
            };
            // Newest first: ISO times sort as text.
            found.sort_by(|one, other| other.0.cmp(&one.0));
            Ok(found.into_iter().map(|(_, issue)| issue).collect())
        })
    }

    fn view<'a>(&'a self, key: &'a TicketKey) -> BoxFuture<'a, Result<IssueDetail, ServiceError>> {
        Box::pin(async move {
            let found = match &self.access {
                Access::Api { .. } => {
                    let data = self.graphql(VIEW, json!({ "id": key.as_str() })).await?;
                    data.get("issue").cloned().unwrap_or(Value::Null)
                }
                Access::Mcp { mcp } => {
                    let mut mcp = mcp.lock().await;
                    let tool = mcp.tool(&["get_issue", "getIssue"])?;
                    let args =
                        tool.args(&[(&["id", "issueId", "identifier"], json!(key.as_str()))]);
                    let answer = mcp.call(&tool, args).await?;
                    answer.get("issue").cloned().unwrap_or(answer)
                }
            };
            read_issue(&found)
                .ok_or_else(|| ServiceError::NotFound(format!("Linear has no issue {key}")))
        })
    }

    fn close<'a>(&'a self, key: &'a TicketKey) -> BoxFuture<'a, Result<(), ServiceError>> {
        Box::pin(async move {
            let not_closable = || {
                ServiceError::Refused(format!(
                    "{key}'s team has no completed state; close it in Linear"
                ))
            };
            match &self.access {
                Access::Api { .. } => {
                    let data = self.graphql(STATES, json!({ "id": key.as_str() })).await?;
                    let states = data
                        .pointer("/issue/team/states/nodes")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default();
                    let (state, _) = completed(&states).ok_or_else(not_closable)?;
                    let data = self
                        .graphql(CLOSE, json!({ "id": key.as_str(), "state": state }))
                        .await?;
                    if data
                        .pointer("/issueUpdate/success")
                        .and_then(Value::as_bool)
                        == Some(true)
                    {
                        Ok(())
                    } else {
                        Err(ServiceError::Refused(format!("Linear did not close {key}")))
                    }
                }
                Access::Mcp { mcp } => {
                    let mut mcp = mcp.lock().await;
                    let list = mcp.tool(&["list_issue_statuses", "listIssueStatuses"])?;
                    let args = list.args(&[(&["team", "teamId", "teamKey"], json!(self.team))]);
                    let answer = mcp.call(&list, args).await?;
                    let states = answer
                        .get("statuses")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_else(|| items(&answer));
                    let (id, name) = completed(&states).ok_or_else(not_closable)?;
                    let update = mcp.tool(&["update_issue", "updateIssue"])?;
                    let takes_id = update.input_schema.pointer("/properties/stateId").is_some();
                    let args = update.args(&[
                        (&["id", "issueId", "identifier"], json!(key.as_str())),
                        (
                            &["state", "stateId", "status"],
                            json!(if takes_id { id } else { name }),
                        ),
                    ]);
                    mcp.call(&update, args).await.map(drop)
                }
            }
        })
    }
}

#[cfg(test)]
mod tests;

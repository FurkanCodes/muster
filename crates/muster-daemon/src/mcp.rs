//! A small MCP client over Streamable HTTP, enough to call a remote MCP
//! server's tools as the person: Atlassian's for Jira, Linear's. JSON-RPC
//! posted to the server's address with the person's access token; the
//! answer comes as JSON or as a stream of server-sent events; the session
//! the server names (`Mcp-Session-Id`) is carried on.
//!
//! Servers' tools change their names between versions, and their argument
//! names are theirs, so a caller names the tools it can use by every name
//! known and fills in arguments by the names a tool's input schema says.

use serde::Deserialize;
use serde_json::{Value, json};

use crate::secrets::Token;
use crate::service::ServiceError;

/// The protocol version Muster speaks first; the server may answer with
/// another it speaks.
const PROTOCOL_VERSION: &str = "2025-06-18";

/// The most of an answer read.
const MAX_ANSWER: usize = 16 * 1024 * 1024;

/// A tool a server offers.
#[derive(Clone, Debug, Deserialize)]
pub(crate) struct Tool {
    pub(crate) name: String,
    #[serde(default, rename = "inputSchema")]
    pub(crate) input_schema: Value,
}

impl Tool {
    /// The names of its arguments.
    fn arguments(&self) -> Vec<&str> {
        self.input_schema
            .get("properties")
            .and_then(Value::as_object)
            .map(|properties| properties.keys().map(String::as_str).collect())
            .unwrap_or_default()
    }

    /// Arguments for it: each value under the first of its names the tool
    /// takes, or the first name if its schema names none of them.
    pub(crate) fn args(&self, wanted: &[(&[&str], Value)]) -> Value {
        let known = self.arguments();
        let mut args = serde_json::Map::new();
        for (names, value) in wanted {
            let name = names
                .iter()
                .find(|name| known.contains(name))
                .or_else(|| names.first());
            if let Some(name) = name {
                args.insert((*name).to_owned(), value.clone());
            }
        }
        Value::Object(args)
    }
}

/// A session with one MCP server.
#[derive(Debug)]
pub(crate) struct Mcp {
    http: reqwest::Client,
    url: String,
    token: Token,
    /// What to call it in messages: `Atlassian`.
    service: &'static str,
    /// How to sign in again, for messages: `muster auth jira`.
    sign_in: &'static str,
    session: Option<String>,
    protocol: String,
    next_id: u64,
    pub(crate) tools: Vec<Tool>,
}

impl Mcp {
    /// Opens a session with the server at `url`: the handshake, then the
    /// tools it offers.
    pub(crate) async fn connect(
        http: reqwest::Client,
        url: &str,
        token: Token,
        service: &'static str,
        sign_in: &'static str,
    ) -> Result<Self, ServiceError> {
        let mut mcp = Self {
            http,
            url: url.to_owned(),
            token,
            service,
            sign_in,
            session: None,
            protocol: PROTOCOL_VERSION.to_owned(),
            next_id: 1,
            tools: Vec::new(),
        };
        let started = mcp
            .request(
                "initialize",
                json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": {},
                    "clientInfo": { "name": "muster", "version": env!("CARGO_PKG_VERSION") },
                }),
            )
            .await?;
        if let Some(version) = started.get("protocolVersion").and_then(Value::as_str) {
            mcp.protocol = version.to_owned();
        }
        mcp.notify("notifications/initialized").await?;
        let mut cursor: Option<String> = None;
        for _ in 0..20 {
            let params = match &cursor {
                Some(cursor) => json!({ "cursor": cursor }),
                None => json!({}),
            };
            let page = mcp.request("tools/list", params).await?;
            if let Some(tools) = page.get("tools") {
                let tools: Vec<Tool> =
                    serde_json::from_value(tools.clone()).map_err(|error| mcp.unreadable(error))?;
                mcp.tools.extend(tools);
            }
            cursor = page
                .get("nextCursor")
                .and_then(Value::as_str)
                .map(str::to_owned);
            if cursor.is_none() {
                break;
            }
        }
        Ok(mcp)
    }

    /// The first tool it offers by any of `names`.
    pub(crate) fn tool(&self, names: &[&str]) -> Result<Tool, ServiceError> {
        names
            .iter()
            .find_map(|name| self.tools.iter().find(|tool| tool.name == *name))
            .cloned()
            .ok_or_else(|| {
                ServiceError::Refused(format!(
                    "{}'s MCP server offers no {} tool (it may have changed, or the sign-in \
                     lacks its permission); a token works instead",
                    self.service,
                    names.first().copied().unwrap_or("such")
                ))
            })
    }

    /// Calls `tool` with `args`: what it answers, as JSON if it answers
    /// JSON (in its structured content, or as its text), else its text.
    pub(crate) async fn call(&mut self, tool: &Tool, args: Value) -> Result<Value, ServiceError> {
        let result = self
            .request(
                "tools/call",
                json!({ "name": tool.name, "arguments": args }),
            )
            .await?;
        let text: String = result
            .get("content")
            .and_then(Value::as_array)
            .map(|content| {
                content
                    .iter()
                    .filter_map(|item| item.get("text").and_then(Value::as_str))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default();
        if result.get("isError").and_then(Value::as_bool) == Some(true) {
            let said: String = muster_core::text::one_line(&text)
                .chars()
                .take(300)
                .collect();
            return Err(ServiceError::Refused(format!(
                "{}'s {} refused: {said}",
                self.service, tool.name
            )));
        }
        if let Some(structured) = result
            .get("structuredContent")
            .filter(|value| !value.is_null())
        {
            return Ok(structured.clone());
        }
        Ok(serde_json::from_str(&text).unwrap_or(Value::String(text)))
    }

    fn unreadable(&self, error: impl std::fmt::Display) -> ServiceError {
        ServiceError::Unreadable(format!(
            "{}'s MCP server answered something unexpected: {error}",
            self.service
        ))
    }

    fn post(&self, body: &Value) -> reqwest::RequestBuilder {
        let mut request = self
            .http
            .post(&self.url)
            .bearer_auth(self.token.value())
            .header("Accept", "application/json, text/event-stream")
            .header("MCP-Protocol-Version", &self.protocol)
            .json(body);
        if let Some(session) = &self.session {
            request = request.header("Mcp-Session-Id", session);
        }
        request
    }

    async fn notify(&mut self, method: &str) -> Result<(), ServiceError> {
        let body = json!({ "jsonrpc": "2.0", "method": method });
        let response = self
            .post(&body)
            .send()
            .await
            .map_err(|error| self.unreachable(error))?;
        self.check(response.status().as_u16(), "")?;
        Ok(())
    }

    fn unreachable(&self, error: impl std::fmt::Display) -> ServiceError {
        ServiceError::Unavailable(format!(
            "could not reach {}'s MCP server: {error}",
            self.service
        ))
    }

    fn check(&self, status: u16, text: &str) -> Result<(), ServiceError> {
        let said: String = muster_core::text::one_line(text)
            .chars()
            .take(300)
            .collect();
        match status {
            200..=299 => Ok(()),
            401 => Err(ServiceError::NotSignedIn(format!(
                "the sign-in to {} has run out or was withdrawn: run `{}` again",
                self.service, self.sign_in
            ))),
            403 => Err(ServiceError::Refused(format!(
                "{} refused (403): {said}",
                self.service
            ))),
            _ => Err(ServiceError::Refused(format!(
                "{}'s MCP server answered {status}: {said}",
                self.service
            ))),
        }
    }

    /// Sends a request and waits for its answer's result.
    async fn request(&mut self, method: &str, params: Value) -> Result<Value, ServiceError> {
        let id = self.next_id;
        self.next_id += 1;
        let body = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        let response = self
            .post(&body)
            .send()
            .await
            .map_err(|error| self.unreachable(error))?;
        if let Some(session) = response
            .headers()
            .get("mcp-session-id")
            .and_then(|value| value.to_str().ok())
        {
            self.session = Some(session.to_owned());
        }
        let status = response.status().as_u16();
        let streamed = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|kind| kind.starts_with("text/event-stream"));
        let bytes = response
            .bytes()
            .await
            .map_err(|error| self.unreachable(error))?;
        let text = String::from_utf8_lossy(&bytes[..bytes.len().min(MAX_ANSWER)]).into_owned();
        self.check(status, &text)?;
        let answer = if streamed {
            answer_in_stream(&text, id).ok_or_else(|| self.unreadable("no answer in its stream"))?
        } else {
            serde_json::from_str(&text).map_err(|error| self.unreadable(error))?
        };
        if let Some(error) = answer.get("error") {
            let said = error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("an error");
            return Err(ServiceError::Refused(format!(
                "{}'s MCP server refused {method}: {}",
                self.service,
                muster_core::text::one_line(said)
            )));
        }
        Ok(answer.get("result").cloned().unwrap_or(Value::Null))
    }
}

/// The answer to request `id` in a stream of server-sent events: the event
/// whose data is a JSON-RPC message with that id.
fn answer_in_stream(text: &str, id: u64) -> Option<Value> {
    let mut data = String::new();
    let mut found = None;
    let mut finish = |data: &mut String| {
        if !data.is_empty() {
            if let Ok(message) = serde_json::from_str::<Value>(data)
                && message.get("id").and_then(Value::as_u64) == Some(id)
            {
                found = Some(message);
            }
            data.clear();
        }
    };
    for line in text.lines() {
        if line.trim().is_empty() {
            finish(&mut data);
        } else if let Some(rest) = line.strip_prefix("data:") {
            if !data.is_empty() {
                data.push('\n');
            }
            data.push_str(rest.strip_prefix(' ').unwrap_or(rest));
        }
    }
    finish(&mut data);
    found
}

#[cfg(test)]
mod tests;

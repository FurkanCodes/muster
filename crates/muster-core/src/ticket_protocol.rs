//! The ticket protocol (`ARCHITECTURE.md` §10, *Ticket plugins*): how the
//! daemon asks a ticket plugin for a project's tickets. JSON-RPC 2.0, one
//! message a line, over the plugin's stdin and stdout: `hello` once, then
//! `list`, `view` and `close`, each naming the project it is about.
//!
//! What a plugin answers is never trusted (`PRD-07`): it is checked and
//! cleaned here before anything else sees it. A ticket's key must be a
//! [`TicketKey`], its text loses its control characters, and everything is
//! cut to a size; a ticket that cannot be taken is dropped and counted, not
//! the listing.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::text::{multi_line, one_line};
use crate::time::{parse_rfc3339, rfc3339};
use crate::{Issue, IssueComment, TicketKey};

/// The version of the protocol this Muster speaks, said in `hello`.
pub const PROTOCOL: u32 = 1;

/// The longest line a plugin may answer with, its newline left out.
pub const MOST_LINE: usize = 4 * 1024 * 1024;

/// The most tickets a listing keeps.
pub const MOST_TICKETS: usize = 1_000;

/// The most of a ticket's title kept, in bytes.
const MOST_TITLE: usize = 1_000;

/// The most of a link kept.
const MOST_URL: usize = 2_048;

/// The most labels a ticket keeps.
const MOST_LABELS: usize = 50;

/// The most comments a ticket keeps: its newest, the oldest cut.
const MOST_COMMENTS: usize = 500;

/// The most of a comment's text kept.
const MOST_COMMENT: usize = 64 * 1024;

/// The most of a name kept: a label, an author, where the tickets are.
const MOST_NAME: usize = 200;

/// The most of a ticket's text kept.
const MOST_BODY: usize = 256 * 1024;

/// The most of a plugin's reason kept.
const MOST_MESSAGE: usize = 4_000;

/// The project a request is about.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Project {
    /// Its folder.
    pub root: PathBuf,
    /// Its git remote as `host/path` (`github.com/o/app`), never the
    /// remote's address, which can carry a password; none where it has no
    /// remote Muster can read.
    #[serde(default)]
    pub remote: Option<String>,
    /// What its `[tickets]` table says for the plugin, by key.
    #[serde(default)]
    pub settings: BTreeMap<String, String>,
}

impl Project {
    /// The project in `root`; none where the folder's name is not UTF-8,
    /// which JSON cannot carry.
    pub fn new(
        root: &Path,
        remote: Option<String>,
        settings: BTreeMap<String, String>,
    ) -> Option<Self> {
        root.to_str()?;
        Some(Self {
            root: root.to_owned(),
            remote,
            settings,
        })
    }
}

/// What the daemon asks a plugin.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "method", content = "params", rename_all = "lowercase")]
pub enum Call {
    /// First, once a process: which Muster asks, and the protocol it
    /// speaks. Answered with the plugin's (`HelloResult`).
    Hello { muster: String, protocol: u32 },
    /// The project's open tickets, newest first; only those assigned to
    /// the person signed in where `mine`. Answered with a `Listing`.
    List {
        project: Project,
        #[serde(default)]
        mine: bool,
    },
    /// One ticket, with its text. Answered with a `Viewing`.
    View { project: Project, key: TicketKey },
    /// Closes a ticket, because the person asked. Answered with `{}`.
    Close { project: Project, key: TicketKey },
}

/// A request, as it goes on the wire.
#[derive(Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
struct Outgoing<'a> {
    jsonrpc: &'static str,
    id: u64,
    #[serde(flatten)]
    call: &'a Call,
}

/// A request a plugin read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub id: u64,
    pub call: Call,
}

/// Request `id`, `call`, as a line.
pub fn request_line(id: u64, call: &Call) -> String {
    line(&Outgoing {
        jsonrpc: "2.0",
        id,
        call,
    })
}

/// A request read from `line`, or why it is none, with its id where it
/// had one, to answer it by.
pub fn read_request(line: &[u8]) -> Result<Request, (Option<u64>, String)> {
    let value: Value = serde_json::from_slice(line)
        .map_err(|error| (None, format!("not a JSON-RPC request: {error}")))?;
    let id = value.get("id").and_then(Value::as_u64);
    let call = serde_json::from_value(value).map_err(|error| (id, error.to_string()))?;
    let id = id.ok_or_else(|| (None, "a request needs a number for its id".to_owned()))?;
    Ok(Request { id, call })
}

/// What a plugin's error says went wrong, as Muster tells it on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum PluginErrorCode {
    /// No such ticket.
    NotFound,
    /// The tracker refused: a permission, a bad query.
    Refused,
    /// The person is not signed in, or their sign-in ran out.
    SignedOut,
    /// The project does not say enough to find its tickets.
    NotConfigured,
    /// Anything else: the tracker could not be reached, the plugin broke.
    Failed,
}

/// Why a plugin could not answer, as it said it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct PluginError {
    pub code: PluginErrorCode,
    pub message: String,
}

/// A plugin's answer to request `id`.
#[derive(Clone, Debug, PartialEq)]
pub struct Reply {
    /// None where the plugin could not read the request it answers.
    pub id: Option<u64>,
    pub outcome: Result<Value, PluginError>,
}

/// A reply as a line is read: any code an error carries is taken, an
/// unknown one, or a number as plain JSON-RPC has them, as `failed`.
#[derive(Deserialize)]
struct Incoming {
    #[serde(default)]
    id: Option<Value>,
    #[serde(default)]
    result: Option<Value>,
    #[serde(default)]
    error: Option<IncomingError>,
}

#[derive(Deserialize)]
struct IncomingError {
    #[serde(default)]
    code: Value,
    #[serde(default)]
    message: String,
}

/// A reply read from `line`, or why the line is none: a plugin that writes
/// anything else on its stdout is broken.
pub fn read_reply(line: &[u8]) -> Result<Reply, String> {
    let unreadable = |error: serde_json::Error| format!("not a JSON-RPC reply: {error}");
    let value: Value = serde_json::from_slice(line).map_err(unreadable)?;
    // An object, so a list is not read as one's fields in order.
    if !value.is_object() {
        return Err("not a JSON-RPC reply: not an object".to_owned());
    }
    let incoming: Incoming = serde_json::from_value(value).map_err(unreadable)?;
    let id = match incoming.id {
        None | Some(Value::Null) => None,
        Some(id) => Some(
            id.as_u64()
                .ok_or_else(|| format!("a reply's id must be a request's, not {id}"))?,
        ),
    };
    let outcome = match (incoming.result, incoming.error) {
        (_, Some(error)) => Err(PluginError {
            code: serde_json::from_value(error.code).unwrap_or(PluginErrorCode::Failed),
            message: cut(multi_line(&error.message), MOST_MESSAGE),
        }),
        (Some(result), None) => Ok(result),
        (None, None) => return Err("a reply needs a result or an error".to_owned()),
    };
    Ok(Reply { id, outcome })
}

/// The answer to request `id`, as a line.
pub fn result_line(id: u64, result: &impl Serialize) -> String {
    #[derive(Serialize)]
    struct Answer<'a, T> {
        jsonrpc: &'static str,
        id: u64,
        result: &'a T,
    }
    line(&Answer {
        jsonrpc: "2.0",
        id,
        result,
    })
}

/// The refusal of request `id` (none, for a request that could not be
/// read), as a line.
pub fn error_line(id: Option<u64>, error: &PluginError) -> String {
    #[derive(Serialize)]
    struct Refusal<'a> {
        jsonrpc: &'static str,
        id: Option<u64>,
        error: &'a PluginError,
    }
    line(&Refusal {
        jsonrpc: "2.0",
        id,
        error,
    })
}

/// `message` as one line of JSON and its newline.
fn line(message: &impl Serialize) -> String {
    // Plain data with text keys: writing it cannot fail.
    let mut line = serde_json::to_string(message).unwrap_or_default();
    line.push('\n');
    line
}

/// What `hello` is answered with.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct HelloResult {
    /// The protocol the plugin speaks.
    pub protocol: u32,
}

/// A ticket as a plugin gives it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Ticket {
    /// `42`, or `APP-7`; a number is taken as one written as text.
    pub key: Key,
    pub title: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub labels: Vec<String>,
    /// Who filed it.
    #[serde(default)]
    pub author: Option<String>,
}

/// What someone said on a ticket, as a plugin gives it: who, when (RFC
/// 3339, `2026-10-22T09:30:00Z`) and what.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Comment {
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub at: Option<String>,
    pub body: String,
}

impl From<&IssueComment> for Comment {
    fn from(comment: &IssueComment) -> Self {
        Self {
            author: comment.author.clone(),
            at: comment.at.map(rfc3339),
            body: comment.body.clone(),
        }
    }
}

/// A ticket's key as a plugin writes it: text, or a plain number.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(untagged)]
pub enum Key {
    Text(String),
    Number(u64),
}

impl From<&TicketKey> for Key {
    fn from(key: &TicketKey) -> Self {
        Self::Text(key.as_str().to_owned())
    }
}

impl From<&Issue> for Ticket {
    fn from(issue: &Issue) -> Self {
        Self {
            key: Key::from(&issue.key),
            title: issue.title.clone(),
            url: issue.url.clone(),
            labels: issue.labels.clone(),
            author: issue.author.clone(),
        }
    }
}

/// What `list` is answered with.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Listing {
    /// Where the tickets are, for the person: `GitHub issues`, `YouTrack
    /// (acme.myjetbrains.com)`.
    #[serde(rename = "where")]
    pub tracker: String,
    /// Where they are, the same for every project listing the same
    /// tickets; none for a repository's own tracker.
    pub source: Option<String>,
    pub tickets: Vec<Ticket>,
}

/// What `view` is answered with.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Viewing {
    pub ticket: Ticket,
    pub body: String,
    /// What was said on it after it was filed, oldest first; a plugin
    /// may leave it out.
    pub comments: Vec<Comment>,
}

/// The protocol a plugin's answer to `hello` says it speaks.
pub fn hello(result: Value) -> Result<u32, String> {
    serde_json::from_value::<HelloResult>(result)
        .map(|hello| hello.protocol)
        .map_err(|error| format!("its hello answered something unexpected: {error}"))
}

/// A listing, checked and cleaned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listed {
    /// Where the tickets are, if the plugin said.
    pub tracker: Option<String>,
    pub source: Option<String>,
    pub tickets: Vec<Issue>,
    /// How many tickets it gave that were not taken: one with a key that is
    /// none or a field of the wrong kind, or past the most a listing keeps.
    pub dropped: usize,
}

#[derive(Deserialize)]
struct RawListing {
    #[serde(rename = "where", default)]
    tracker: Option<String>,
    #[serde(default)]
    source: Option<String>,
    tickets: Vec<Value>,
}

/// The answer to `list`, checked and cleaned.
pub fn listed(result: Value) -> Result<Listed, String> {
    let raw: RawListing = serde_json::from_value(result)
        .map_err(|error| format!("its listing is not one: {error}"))?;
    let given = raw.tickets.len();
    let tickets: Vec<Issue> = raw
        .tickets
        .into_iter()
        .filter_map(ticket)
        .take(MOST_TICKETS)
        .collect();
    Ok(Listed {
        tracker: name(raw.tracker),
        source: name(raw.source),
        dropped: given - tickets.len(),
        tickets,
    })
}

/// A ticket with its text, checked and cleaned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Viewed {
    pub issue: Issue,
    pub body: String,
    pub comments: Vec<IssueComment>,
}

#[derive(Deserialize)]
struct RawViewing {
    ticket: Value,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    comments: Option<Value>,
}

/// The answer to `view` for `asked`, checked and cleaned: the ticket must be
/// the one asked for.
pub fn viewed(result: Value, asked: &TicketKey) -> Result<Viewed, String> {
    let raw: RawViewing = serde_json::from_value(result)
        .map_err(|error| format!("its ticket is not one: {error}"))?;
    let issue = ticket(raw.ticket).ok_or("its ticket has no key Muster can take, or no title")?;
    if issue.key != *asked {
        return Err(format!("asked for {asked}, it answered with {}", issue.key));
    }
    let mut comments: Vec<IssueComment> = match raw.comments {
        Some(Value::Array(comments)) => comments.into_iter().filter_map(comment).collect(),
        _ => Vec::new(),
    };
    if comments.len() > MOST_COMMENTS {
        comments.drain(..comments.len() - MOST_COMMENTS);
    }
    Ok(Viewed {
        issue,
        body: cut(multi_line(&raw.body.unwrap_or_default()), MOST_BODY),
        comments,
    })
}

/// `value` as a comment Muster takes, cleaned; none where a field is of the
/// wrong kind or it says nothing. A moment that does not read is left out,
/// not the comment.
fn comment(value: Value) -> Option<IssueComment> {
    let comment: Comment = serde_json::from_value(value).ok()?;
    let body = cut(multi_line(&comment.body), MOST_COMMENT);
    if body.trim().is_empty() {
        return None;
    }
    Some(IssueComment {
        author: name(comment.author),
        at: comment.at.as_deref().and_then(parse_rfc3339),
        body,
    })
}

/// `value` as a ticket Muster takes, cleaned; none where its key is not a
/// `TicketKey` or a field is of the wrong kind.
fn ticket(value: Value) -> Option<Issue> {
    let ticket: Ticket = serde_json::from_value(value).ok()?;
    let key = match ticket.key {
        Key::Text(text) => TicketKey::parse(&text)?,
        Key::Number(number) => TicketKey::number(number),
    };
    Some(Issue {
        key,
        title: cut(one_line(&ticket.title), MOST_TITLE),
        url: cut(one_line(&ticket.url), MOST_URL),
        labels: ticket
            .labels
            .iter()
            .take(MOST_LABELS)
            .map(|label| cut(one_line(label), MOST_NAME))
            .collect(),
        author: name(ticket.author),
    })
}

/// A name a plugin gave, on one line and cut; none where it is empty.
fn name(text: Option<String>) -> Option<String> {
    text.map(|text| cut(one_line(&text).trim().to_owned(), MOST_NAME))
        .filter(|text| !text.is_empty())
}

/// `text` cut to at most `most` bytes, on a character's boundary.
fn cut(mut text: String, most: usize) -> String {
    if text.len() > most {
        let end = (0..=most)
            .rev()
            .find(|index| text.is_char_boundary(*index))
            .unwrap_or(0);
        text.truncate(end);
    }
    text
}

#[cfg(test)]
mod tests;

//! Muster's own ticket plugins, served (`muster plugin-run github|gitlab|
//! jira|linear`, `ARCHITECTURE.md` §10, *Ticket plugins*): the ticket
//! protocol on stdin and stdout around the trackers in `trackers`, GitHub's
//! through the person's `gh`, GitLab's through their `glab`, Jira's and
//! Linear's through their MCP servers or APIs, signed in as `muster auth`
//! left them: in the Keychain, or a token in the environment, which is the
//! daemon's as sessions get it. The daemon runs them as it would anyone's
//! (`trackers::plugin`). Each request names its project, with what its
//! `[tickets]` says, and a listing whether only the person's own are
//! wanted; each is answered as it finishes, so a slow one holds up no
//! other. It ends when its stdin does, once what is under way is answered.

use std::sync::Arc;
use std::time::Duration;

use muster_core::config::Tickets;
use muster_core::plugins::own;
use muster_core::ticket_protocol::{
    self, Call, HelloResult, Listing, MOST_LINE, PROTOCOL, PluginError, PluginErrorCode, Project,
    Request, Ticket, Viewing,
};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio::task::JoinSet;

use crate::secrets::{Keychain, Secrets};
use crate::service::{BoxFuture, ServiceError};
use crate::trackers::TicketSource;
use crate::trackers::github::GitHubIssues;
use crate::trackers::gitlab::GitLabIssues;
use crate::trackers::jira::Jira;
use crate::trackers::linear::Linear;
use crate::trackers::plugin::streams::Lines;

/// The most requests answered at once; more wait in the pipe.
const MOST_AT_ONCE: usize = 16;

/// How long one of Jira's or Linear's calls may take: less than the daemon
/// gives a request (`Limits::request`, 30 s), so a tracker out of reach is
/// said to be, not taken for this plugin hanging.
const CALL_LIMIT: Duration = Duration::from_secs(20);

/// Why `plugin-run` could not serve.
#[derive(Debug, thiserror::Error)]
pub enum PluginRunError {
    #[error(
        "Muster has no ticket plugin of its own called {0:?}; its own are github, gitlab, jira \
         and linear"
    )]
    Unknown(String),
    #[error("lost touch with the daemon: {0}")]
    Io(#[from] std::io::Error),
}

/// A project's tickets, made.
type Made = BoxFuture<'static, Result<Arc<dyn TicketSource>, ServiceError>>;

/// Makes a project's tickets, all or only the person's own, or says why
/// it cannot: not signed in, not told which.
type Sources = dyn Fn(Project, bool) -> Made + Send + Sync;

/// Serves Muster's own ticket plugin `id` on stdin and stdout, until stdin
/// ends.
pub async fn plugin_run(id: &str) -> Result<(), PluginRunError> {
    let sources: Arc<Sources> = match id {
        own::GITHUB => Arc::new(|project: Project, mine| {
            made(GitHubIssues {
                root: project.root,
                mine,
            })
        }),
        own::GITLAB => Arc::new(|project: Project, mine| {
            made(GitLabIssues {
                root: project.root,
                mine,
            })
        }),
        own::JIRA => signed_in(own::JIRA),
        own::LINEAR => signed_in(own::LINEAR),
        other => return Err(PluginRunError::Unknown(other.to_owned())),
    };
    serve(sources, tokio::io::stdin(), tokio::io::stdout()).await?;
    Ok(())
}

/// `source`, made at once: its program signs in on its own.
fn made(source: impl TicketSource + 'static) -> Made {
    let source: Arc<dyn TicketSource> = Arc::new(source);
    Box::pin(std::future::ready(Ok(source)))
}

/// Jira's or Linear's tickets, `id`, as the project's `[tickets]` says,
/// signed in as the person.
fn signed_in(id: &'static str) -> Arc<Sources> {
    let secrets: Arc<dyn Secrets> = Arc::new(Keychain::default());
    let http = crate::service::http_client_within(CALL_LIMIT);
    Arc::new(move |project: Project, mine| -> Made {
        let (secrets, http) = (secrets.clone(), http.clone());
        Box::pin(async move {
            let tickets = Tickets {
                tracker: Some(id.to_owned()),
                fields: project.settings,
                mine,
            };
            let source: Arc<dyn TicketSource> = if id == own::JIRA {
                Jira::for_settings(&tickets, secrets.as_ref(), http).await?
            } else {
                Linear::for_settings(&tickets, secrets.as_ref(), http).await?
            };
            Ok(source)
        })
    })
}

/// Answers the requests read from `input` on `output`, until `input` ends.
async fn serve(
    sources: Arc<Sources>,
    input: impl AsyncRead + Unpin,
    mut output: impl AsyncWrite + Unpin,
) -> std::io::Result<()> {
    let mut lines = Lines::new(input, MOST_LINE);
    // Each request's answer is owned here, and ends with it (`ARC-13`).
    let mut answering = JoinSet::new();
    let mut reading = true;
    // Once its stdin ends, what is under way is answered still.
    while reading || !answering.is_empty() {
        tokio::select! {
            line = lines.next(), if reading && answering.len() < MOST_AT_ONCE => match line? {
                None => reading = false,
                Some(line) if line.trim_ascii().is_empty() => {}
                Some(line) => match ticket_protocol::read_request(&line) {
                    Ok(request) => {
                        answering.spawn(answer(sources.clone(), request));
                    }
                    Err((id, why)) => {
                        let refusal = PluginError {
                            code: PluginErrorCode::Failed,
                            message: format!("could not read the request: {why}"),
                        };
                        write(&mut output, &ticket_protocol::error_line(id, &refusal)).await?;
                    }
                },
            },
            Some(answered) = answering.join_next() => match answered {
                Ok(line) => write(&mut output, &line).await?,
                // Answered by no one: the daemon's deadline answers it.
                Err(error) => eprintln!("muster plugin-run: a request failed: {error}"),
            },
        }
    }
    Ok(())
}

/// The answer to `request`, as a line.
async fn answer(sources: Arc<Sources>, Request { id, call }: Request) -> String {
    let answered = match call {
        Call::Hello { .. } => {
            return ticket_protocol::result_line(id, &HelloResult { protocol: PROTOCOL });
        }
        Call::List { project, mine } => {
            async {
                let source = sources(project, mine).await?;
                let issues = source.list().await?;
                let listing = Listing {
                    tracker: source.describe(),
                    source: source.source(),
                    tickets: issues.iter().map(Ticket::from).collect(),
                };
                Ok(ticket_protocol::result_line(id, &listing))
            }
            .await
        }
        Call::View { project, key } => {
            async {
                let detail = sources(project, false).await?.view(&key).await?;
                let viewing = Viewing {
                    ticket: Ticket::from(&detail.issue),
                    body: detail.body,
                };
                Ok(ticket_protocol::result_line(id, &viewing))
            }
            .await
        }
        Call::Close { project, key } => {
            async {
                sources(project, false).await?.close(&key).await?;
                Ok(ticket_protocol::result_line(id, &serde_json::json!({})))
            }
            .await
        }
    };
    answered.unwrap_or_else(|error: ServiceError| {
        ticket_protocol::error_line(Some(id), &plugin_error(&error))
    })
}

/// Muster's error as the protocol carries it: the daemon makes the same
/// of it again (`trackers::plugin::refusal`).
fn plugin_error(error: &ServiceError) -> PluginError {
    let code = match error {
        ServiceError::NotConfigured(_) => PluginErrorCode::NotConfigured,
        ServiceError::NotSignedIn(_) => PluginErrorCode::SignedOut,
        ServiceError::NotFound(_) => PluginErrorCode::NotFound,
        ServiceError::Refused(_) => PluginErrorCode::Refused,
        ServiceError::Unavailable(_) | ServiceError::Unreadable(_) => PluginErrorCode::Failed,
    };
    PluginError {
        code,
        message: error.to_string(),
    }
}

async fn write(output: &mut (impl AsyncWrite + Unpin), line: &str) -> std::io::Result<()> {
    output.write_all(line.as_bytes()).await?;
    output.flush().await
}

#[cfg(test)]
mod tests;

//! Muster's own ticket plugins, served (`muster plugin-run github|gitlab`,
//! `ARCHITECTURE.md` §10, *Ticket plugins*): the ticket protocol on stdin
//! and stdout around the trackers in `trackers`, GitHub's through the
//! person's `gh` and GitLab's through their `glab`. The daemon runs them
//! as it would anyone's (`trackers::plugin`). Each request names its
//! project, and a listing whether only the person's own are wanted; each
//! is answered as it finishes, so a slow one holds up no other. It ends
//! when its stdin does, once what is under way is answered.

use std::path::PathBuf;
use std::sync::Arc;

use muster_core::ticket_protocol::{
    self, Call, HelloResult, Listing, MOST_LINE, PROTOCOL, PluginError, PluginErrorCode, Request,
    Ticket, Viewing,
};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio::task::JoinSet;

use crate::service::ServiceError;
use crate::trackers::TicketSource;
use crate::trackers::github::GitHubIssues;
use crate::trackers::gitlab::GitLabIssues;
use crate::trackers::plugin::streams::Lines;

/// The most requests answered at once; more wait in the pipe.
const MOST_AT_ONCE: usize = 16;

/// Why `plugin-run` could not serve.
#[derive(Debug, thiserror::Error)]
pub enum PluginRunError {
    #[error("Muster has no ticket plugin of its own called {0:?}; its own are github and gitlab")]
    Unknown(String),
    #[error("lost touch with the daemon: {0}")]
    Io(#[from] std::io::Error),
}

/// Makes the tickets of the repository in a folder, all or only the
/// person's own.
type Sources = dyn Fn(PathBuf, bool) -> Arc<dyn TicketSource> + Send + Sync;

/// Serves Muster's own ticket plugin `id` on stdin and stdout, until stdin
/// ends.
pub async fn plugin_run(id: &str) -> Result<(), PluginRunError> {
    let sources: Arc<Sources> = match id {
        "github" => Arc::new(|root, mine| -> Arc<dyn TicketSource> {
            Arc::new(GitHubIssues { root, mine })
        }),
        "gitlab" => Arc::new(|root, mine| -> Arc<dyn TicketSource> {
            Arc::new(GitLabIssues { root, mine })
        }),
        other => return Err(PluginRunError::Unknown(other.to_owned())),
    };
    serve(sources, tokio::io::stdin(), tokio::io::stdout()).await?;
    Ok(())
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
            let source = sources(project.root, mine);
            source.list().await.map(|issues| {
                let listing = Listing {
                    tracker: source.describe(),
                    source: source.source(),
                    tickets: issues.iter().map(Ticket::from).collect(),
                };
                ticket_protocol::result_line(id, &listing)
            })
        }
        Call::View { project, key } => {
            sources(project.root, false).view(&key).await.map(|detail| {
                let viewing = Viewing {
                    ticket: Ticket::from(&detail.issue),
                    body: detail.body,
                };
                ticket_protocol::result_line(id, &viewing)
            })
        }
        Call::Close { project, key } => sources(project.root, false)
            .close(&key)
            .await
            .map(|()| ticket_protocol::result_line(id, &serde_json::json!({}))),
    };
    answered.unwrap_or_else(|error| ticket_protocol::error_line(Some(id), &plugin_error(&error)))
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

//! The hook receiver: where agents' hooks post their payloads
//! (`ARCHITECTURE.md` §5), each harness at `/hooks/<harness>`, and one that
//! names its events only in the address at `/hooks/<harness>/<event>`.
//!
//! It listens on its own Unix socket in the private runtime directory, so it
//! is reachable only by this user and never over the network. It does not
//! decide which session a hook belongs to, nor read it: it passes the bearer
//! token and the body, read as JSON, along, and the Supervisor, which issued
//! the tokens, looks the session up (`ARC-11`) and has its harness read it. It
//! answers at once whatever happens, so a hook never holds up an agent.

use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::Router;
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::routing::post;
use tokio::net::UnixListener;
use tokio::sync::mpsc;

/// Payloads carry tool input and output, which can be whole files.
const MAX_BODY: usize = 32 * 1024 * 1024;

/// The most of hooks' bodies waiting on the Supervisor at once (`ARC-14`):
/// past it a report is dropped, as one past the queue's length is.
const MOST_WAITING: usize = 64 * 1024 * 1024;

/// One accepted hook post.
#[derive(Debug)]
pub(crate) struct HookDelivery {
    pub(crate) token: crate::secrets::Token,
    /// The harness the address names: a session's token is good only for
    /// its own harness's reports.
    pub(crate) harness: String,
    /// The event the address names, for a harness that names it there.
    pub(crate) event: Option<String>,
    /// The body, read as JSON here, off the Supervisor's loop, once; `null`
    /// where it is none.
    pub(crate) payload: serde_json::Value,
    /// Its body's bytes counted among those waiting, until it is handled.
    _waiting: Waiting,
}

/// A body's bytes, counted among those waiting while it is held.
#[derive(Debug)]
struct Waiting {
    bytes: usize,
    count: Arc<AtomicUsize>,
}

impl Waiting {
    /// `bytes` more waiting, unless that would be past [`MOST_WAITING`].
    fn take(count: &Arc<AtomicUsize>, bytes: usize) -> Option<Self> {
        if count.fetch_add(bytes, Ordering::Relaxed) + bytes > MOST_WAITING {
            count.fetch_sub(bytes, Ordering::Relaxed);
            return None;
        }
        Some(Self {
            bytes,
            count: count.clone(),
        })
    }
}

impl Drop for Waiting {
    fn drop(&mut self) {
        self.count.fetch_sub(self.bytes, Ordering::Relaxed);
    }
}

/// Where accepted hooks go, and how much of them waits.
#[derive(Clone)]
struct Receiving {
    deliveries: mpsc::Sender<HookDelivery>,
    waiting: Arc<AtomicUsize>,
}

/// Serves hooks on `listener` until `shutdown` completes.
pub(crate) async fn serve(
    listener: UnixListener,
    deliveries: mpsc::Sender<HookDelivery>,
    shutdown: impl Future<Output = ()> + Send + 'static,
) {
    let app = Router::new()
        .route("/hooks/{harness}", post(hook))
        .route("/hooks/{harness}/{event}", post(hook_with_event))
        .layer(DefaultBodyLimit::max(MAX_BODY))
        .with_state(Receiving {
            deliveries,
            waiting: Arc::default(),
        });
    if let Err(error) = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await
    {
        tracing::error!(%error, "the hook receiver stopped");
    }
}

async fn hook(
    State(receiving): State<Receiving>,
    Path(harness): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> StatusCode {
    deliver(&receiving, &headers, harness, None, &body)
}

async fn hook_with_event(
    State(receiving): State<Receiving>,
    Path((harness, event)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> StatusCode {
    deliver(&receiving, &headers, harness, Some(event), &body)
}

fn deliver(
    receiving: &Receiving,
    headers: &HeaderMap,
    harness: String,
    event: Option<String>,
    body: &[u8],
) -> StatusCode {
    let Some(token) = bearer(headers) else {
        return StatusCode::UNAUTHORIZED;
    };
    let Some(waiting) = Waiting::take(&receiving.waiting, body.len()) else {
        tracing::warn!("dropped a hook: too much waits on the daemon");
        return StatusCode::NO_CONTENT;
    };
    let delivery = HookDelivery {
        token: crate::secrets::Token::new(token),
        harness,
        event,
        payload: serde_json::from_slice(body).unwrap_or(serde_json::Value::Null),
        _waiting: waiting,
    };
    if receiving.deliveries.try_send(delivery).is_err() {
        tracing::warn!("dropped a hook: the daemon is behind");
    }
    StatusCode::NO_CONTENT
}

fn bearer(headers: &HeaderMap) -> Option<String> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let token = value.strip_prefix("Bearer ")?.trim();
    (!token.is_empty()).then(|| token.to_owned())
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    #[test]
    fn bodies_waiting_past_the_bound_are_dropped_and_let_go_as_handled() {
        let count = Arc::default();
        let held = Waiting::take(&count, MOST_WAITING).unwrap();
        assert!(Waiting::take(&count, 1).is_none(), "past the bound");
        drop(held);
        assert!(Waiting::take(&count, 1).is_some(), "handled, it makes room");
    }

    #[test]
    fn only_a_bearer_token_is_accepted() {
        let mut headers = HeaderMap::new();
        assert_eq!(bearer(&headers), None);
        headers.insert(header::AUTHORIZATION, HeaderValue::from_static("Basic abc"));
        assert_eq!(bearer(&headers), None);
        headers.insert(header::AUTHORIZATION, HeaderValue::from_static("Bearer "));
        assert_eq!(bearer(&headers), None);
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer abc123"),
        );
        assert_eq!(bearer(&headers).as_deref(), Some("abc123"));
    }
}

//! The hook receiver: where agents' hooks post their payloads
//! (`ARCHITECTURE.md` §5), each harness at `/hooks/<harness>`, and one that
//! names its events only in the address at `/hooks/<harness>/<event>`.
//!
//! It listens on its own Unix socket in the private runtime directory, so it
//! is reachable only by this user and never over the network. It does not
//! decide which session a hook belongs to, nor read it: it passes the bearer
//! token and the body along, and the Supervisor, which issued the tokens,
//! looks the session up (`ARC-11`) and has its harness read the body. It
//! answers at once whatever happens, so a hook never holds up an agent.

use std::future::Future;

use axum::Router;
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::routing::post;
use tokio::net::UnixListener;
use tokio::sync::mpsc;

/// Payloads carry tool input and output, which can be whole files.
const MAX_BODY: usize = 32 * 1024 * 1024;

/// One accepted hook post.
#[derive(Debug)]
pub(crate) struct HookDelivery {
    pub(crate) token: String,
    /// The harness the address names: a session's token is good only for
    /// its own harness's reports.
    pub(crate) harness: String,
    /// The event the address names, for a harness that names it there.
    pub(crate) event: Option<String>,
    pub(crate) body: Bytes,
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
        .with_state(deliveries);
    if let Err(error) = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await
    {
        tracing::error!(%error, "the hook receiver stopped");
    }
}

async fn hook(
    State(deliveries): State<mpsc::Sender<HookDelivery>>,
    Path(harness): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> StatusCode {
    deliver(&deliveries, &headers, harness, None, body)
}

async fn hook_with_event(
    State(deliveries): State<mpsc::Sender<HookDelivery>>,
    Path((harness, event)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> StatusCode {
    deliver(&deliveries, &headers, harness, Some(event), body)
}

fn deliver(
    deliveries: &mpsc::Sender<HookDelivery>,
    headers: &HeaderMap,
    harness: String,
    event: Option<String>,
    body: Bytes,
) -> StatusCode {
    let Some(token) = bearer(headers) else {
        return StatusCode::UNAUTHORIZED;
    };
    let delivery = HookDelivery {
        token,
        harness,
        event,
        body,
    };
    if deliveries.try_send(delivery).is_err() {
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

//! The hook receiver: where agents' hooks post their payloads
//! (`ARCHITECTURE.md` §5).
//!
//! It listens on its own Unix socket in the private runtime directory, so it
//! is reachable only by this user and never over the network. It does not
//! decide which session a hook belongs to: it passes the bearer token along
//! and the Supervisor, which issued the tokens, looks it up (`ARC-11`). It
//! answers at once whatever happens, so a hook never holds up an agent.

use std::future::Future;

use axum::Router;
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::routing::post;
use tokio::net::UnixListener;
use tokio::sync::mpsc;

use crate::harness::claude::HookPayload;

/// Payloads carry tool input and output, which can be whole files.
const MAX_BODY: usize = 32 * 1024 * 1024;

/// One accepted hook post.
#[derive(Debug)]
pub(crate) struct HookDelivery {
    pub(crate) token: String,
    pub(crate) payload: HookPayload,
}

/// Serves hooks on `listener` until `shutdown` completes.
pub(crate) async fn serve(
    listener: UnixListener,
    deliveries: mpsc::Sender<HookDelivery>,
    shutdown: impl Future<Output = ()> + Send + 'static,
) {
    let app = Router::new()
        .route("/hooks/claude", post(claude))
        .layer(DefaultBodyLimit::max(MAX_BODY))
        .with_state(deliveries);
    if let Err(error) = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await
    {
        tracing::error!(%error, "the hook receiver stopped");
    }
}

async fn claude(
    State(deliveries): State<mpsc::Sender<HookDelivery>>,
    headers: HeaderMap,
    body: Bytes,
) -> StatusCode {
    let Some(token) = bearer(&headers) else {
        return StatusCode::UNAUTHORIZED;
    };
    match HookPayload::parse(&body) {
        Some(payload) => {
            if deliveries
                .try_send(HookDelivery { token, payload })
                .is_err()
            {
                tracing::warn!("dropped a hook: the daemon is behind");
            }
        }
        None => tracing::debug!("ignored a hook body that is not a JSON object"),
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

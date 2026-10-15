//! Signing in to an MCP server the way MCP clients do (OAuth 2.1, the MCP
//! authorization spec): the person signs in in their browser, as they do
//! for Claude Code's connectors, and Muster never sees a password or makes
//! them create a token.
//!
//! - Discovery: the server's protected-resource metadata (RFC 9728) names
//!   its authorization server, whose metadata (RFC 8414) gives the rest.
//! - Muster registers itself as a public client (RFC 7591): no secret,
//!   a loopback redirect (`http://127.0.0.1:<port>/callback`).
//! - The code comes back to that loopback address, bound to this sign-in
//!   by a random `state`, and is exchanged with a PKCE verifier (S256).
//! - The session (client id, tokens, expiry) is kept in the Keychain
//!   (`secrets`); an access token near its end is refreshed, the refresh
//!   token rotated, and the session kept again.

use std::time::Duration;

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use sha2::Digest as _;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::secrets::{Secrets, Token};
use crate::service::ServiceError;

/// How long the person has to sign in in their browser.
const SIGN_IN_LIMIT: Duration = Duration::from_secs(300);

/// How long before its end an access token is refreshed.
const REFRESH_AHEAD_SECONDS: u64 = 120;

/// A service signed in to this way.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Provider {
    /// For the person: `Atlassian`.
    pub(crate) name: &'static str,
    /// The MCP server's address, which the tokens are for (RFC 8707).
    pub(crate) resource: String,
    /// The Keychain account its session is kept under.
    pub(crate) account: &'static str,
    /// What Muster asks to do: the least that reads tickets and closes
    /// them, and `offline_access`, for a refresh token, where the server
    /// names it so.
    pub(crate) scopes: &'static [&'static str],
}

/// A sign-in, as kept.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Session {
    pub(crate) client_id: String,
    #[serde(default)]
    pub(crate) client_secret: Option<String>,
    pub(crate) token_endpoint: String,
    pub(crate) resource: String,
    pub(crate) access_token: String,
    #[serde(default)]
    pub(crate) refresh_token: Option<String>,
    /// When the access token ends, in seconds since 1970.
    #[serde(default)]
    pub(crate) expires_at: Option<u64>,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("client_id", &self.client_id)
            .field("token_endpoint", &self.token_endpoint)
            .field("resource", &self.resource)
            .field("expires_at", &self.expires_at)
            .finish_non_exhaustive()
    }
}

impl Session {
    /// Whether the access token is to be refreshed at `now`.
    fn needs_refresh(&self, now: u64) -> bool {
        self.refresh_token.is_some()
            && self
                .expires_at
                .is_some_and(|end| end <= now + REFRESH_AHEAD_SECONDS)
    }
}

/// The authorization server's endpoints.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[expect(
    clippy::struct_field_names,
    reason = "OAuth's own names for them (RFC 8414), as served"
)]
pub(crate) struct Metadata {
    pub(crate) authorization_endpoint: String,
    pub(crate) token_endpoint: String,
    #[serde(default)]
    pub(crate) registration_endpoint: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ResourceMetadata {
    #[serde(default)]
    authorization_servers: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Registered {
    client_id: String,
    #[serde(default)]
    client_secret: Option<Token>,
}

#[derive(Debug, Deserialize)]
struct Tokens {
    access_token: Token,
    #[serde(default)]
    refresh_token: Option<Token>,
    #[serde(default)]
    expires_in: Option<u64>,
}

/// Seconds since the Unix epoch, as tokens' expiry is told.
fn now() -> u64 {
    use crate::clock::Clock;
    u64::try_from(crate::clock::SystemClock.now().as_millis()).unwrap_or(0) / 1000
}

fn unavailable(what: &str, error: impl std::fmt::Display) -> ServiceError {
    ServiceError::Unavailable(format!("could not {what}: {error}"))
}

/// The well-known addresses to try for `url`'s metadata `name`: with the
/// path after it (RFC 8414 and 9728's form), then at the root.
fn well_known(url: &reqwest::Url, name: &str) -> Vec<String> {
    let origin = url.origin().ascii_serialization();
    let path = url.path().trim_end_matches('/');
    let mut addresses = Vec::new();
    if !path.is_empty() {
        addresses.push(format!("{origin}/.well-known/{name}{path}"));
    }
    addresses.push(format!("{origin}/.well-known/{name}"));
    addresses
}

/// The first of `addresses` that answers with JSON of type `T`.
async fn first_json<T: serde::de::DeserializeOwned>(
    http: &reqwest::Client,
    addresses: &[String],
) -> Option<T> {
    for address in addresses {
        let Ok(response) = http.get(address).send().await else {
            continue;
        };
        if response.status().is_success()
            && let Ok(found) = response.json::<T>().await
        {
            return Some(found);
        }
    }
    None
}

/// Finds the authorization server for the MCP server at `resource`.
pub(crate) async fn discover(
    http: &reqwest::Client,
    resource: &str,
) -> Result<Metadata, ServiceError> {
    let url = reqwest::Url::parse(resource)
        .map_err(|error| ServiceError::NotConfigured(format!("{resource}: {error}")))?;
    let issuer =
        first_json::<ResourceMetadata>(http, &well_known(&url, "oauth-protected-resource"))
            .await
            .and_then(|found| found.authorization_servers.into_iter().next())
            .unwrap_or_else(|| url.origin().ascii_serialization());
    let issuer = reqwest::Url::parse(&issuer)
        .map_err(|error| ServiceError::Unreadable(format!("{issuer}: {error}")))?;
    let mut addresses = well_known(&issuer, "oauth-authorization-server");
    addresses.extend(well_known(&issuer, "openid-configuration"));
    first_json::<Metadata>(http, &addresses)
        .await
        .ok_or_else(|| {
            ServiceError::Unavailable(format!(
                "{} does not say how to sign in to it (no OAuth metadata)",
                issuer.origin().ascii_serialization()
            ))
        })
}

/// Registers Muster as a public client that comes back to `redirect`.
async fn register(
    http: &reqwest::Client,
    metadata: &Metadata,
    redirect: &str,
    scopes: &[&str],
) -> Result<Registered, ServiceError> {
    let endpoint = metadata.registration_endpoint.as_deref().ok_or_else(|| {
        ServiceError::Refused(
            "the service does not let Muster register to sign in; use a token instead".to_owned(),
        )
    })?;
    let response = http
        .post(endpoint)
        .json(&serde_json::json!({
            "client_name": "Muster",
            "redirect_uris": [redirect],
            "grant_types": ["authorization_code", "refresh_token"],
            "response_types": ["code"],
            "token_endpoint_auth_method": "none",
            "scope": scopes.join(" "),
        }))
        .send()
        .await
        .map_err(|error| unavailable("register to sign in", error))?;
    if !response.status().is_success() {
        let status = response.status();
        let said = muster_core::text::one_line(&response.text().await.unwrap_or_default());
        return Err(ServiceError::Refused(format!(
            "the service would not let Muster register ({status}): {}; use a token instead",
            said.chars().take(200).collect::<String>()
        )));
    }
    response
        .json()
        .await
        .map_err(|error| ServiceError::Unreadable(format!("registration: {error}")))
}

/// Random bytes, as URL-safe text.
fn random(bytes: usize) -> std::io::Result<String> {
    use std::io::Read;
    let mut buffer = vec![0; bytes];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut buffer)?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buffer))
}

/// A PKCE verifier's challenge (S256).
fn challenge(verifier: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(sha2::Sha256::digest(verifier))
}

/// Where the person signs in.
fn authorize_url(
    metadata: &Metadata,
    client_id: &str,
    redirect: &str,
    verifier: &str,
    state: &str,
    (resource, scopes): (&str, &[&str]),
) -> Result<String, ServiceError> {
    let mut url = reqwest::Url::parse(&metadata.authorization_endpoint)
        .map_err(|error| ServiceError::Unreadable(format!("authorization address: {error}")))?;
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", redirect)
        .append_pair("code_challenge", &challenge(verifier))
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", state)
        .append_pair("resource", resource);
    if !scopes.is_empty() {
        url.query_pairs_mut()
            .append_pair("scope", &scopes.join(" "));
    }
    Ok(url.into())
}

/// Asks the token endpoint for tokens with `form`.
async fn tokens(
    http: &reqwest::Client,
    endpoint: &str,
    form: &[(&str, &str)],
) -> Result<Tokens, ServiceError> {
    let response = http
        .post(endpoint)
        .form(form)
        .send()
        .await
        .map_err(|error| unavailable("reach the sign-in service", error))?;
    let status = response.status();
    if !status.is_success() {
        let said = muster_core::text::one_line(&response.text().await.unwrap_or_default());
        let said: String = said.chars().take(200).collect();
        return Err(if status.as_u16() == 400 || status.as_u16() == 401 {
            ServiceError::NotSignedIn(format!("the sign-in was not accepted ({status}): {said}"))
        } else {
            ServiceError::Refused(format!("the sign-in service answered {status}: {said}"))
        });
    }
    response
        .json()
        .await
        .map_err(|error| ServiceError::Unreadable(format!("tokens: {error}")))
}

/// The code the browser brought back to the loopback address, if this
/// request is that: `GET /callback?code=…&state=…`.
fn callback_code(request_line: &str, state: &str) -> Option<Result<String, String>> {
    let target = request_line.split_whitespace().nth(1)?;
    let url = reqwest::Url::parse(&format!("http://127.0.0.1{target}")).ok()?;
    if url.path() != "/callback" {
        return None;
    }
    let mut code = None;
    let (mut came, mut error) = (None, None);
    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "code" => code = Some(value.into_owned()),
            "state" => came = Some(value.into_owned()),
            "error" => error = Some(value.into_owned()),
            _ => {}
        }
    }
    if came.as_deref() != Some(state) {
        return Some(Err("the sign-in came back for another request".to_owned()));
    }
    Some(match (code, error) {
        (Some(code), None) => Ok(code),
        (_, Some(error)) => Err(format!("the sign-in was refused: {error}")),
        (None, None) => Err("the sign-in came back without a code".to_owned()),
    })
}

/// Waits on `listener` for the browser to come back with the code.
async fn wait_for_code(
    listener: tokio::net::TcpListener,
    state: &str,
) -> Result<String, ServiceError> {
    let work = async {
        loop {
            let (mut stream, _) = listener
                .accept()
                .await
                .map_err(|error| unavailable("wait for the sign-in", error))?;
            let mut buffer = vec![0; 8192];
            let read = stream.read(&mut buffer).await.unwrap_or(0);
            let request = String::from_utf8_lossy(&buffer[..read]).into_owned();
            let Some(outcome) = callback_code(request.lines().next().unwrap_or_default(), state)
            else {
                let _ = stream
                    .write_all(
                        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    )
                    .await;
                continue;
            };
            let page = match &outcome {
                Ok(_) => "Signed in to Muster. You can close this window.",
                Err(_) => "Muster's sign-in did not complete. You can close this window.",
            };
            let html = format!("<!doctype html><title>Muster</title><p>{page}</p>");
            let _ = stream
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{html}",
                        html.len()
                    )
                    .as_bytes(),
                )
                .await;
            return outcome.map_err(ServiceError::Refused);
        }
    };
    tokio::time::timeout(SIGN_IN_LIMIT, work)
        .await
        .map_err(|_| ServiceError::NotSignedIn("the sign-in took too long; try again".to_owned()))?
}

/// Signs the person in to `provider` in their browser (`open` shows them
/// the address) and keeps the session.
pub(crate) async fn sign_in(
    http: &reqwest::Client,
    secrets: &dyn Secrets,
    provider: &Provider,
    open: impl FnOnce(&str),
) -> Result<(), ServiceError> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|error| unavailable("listen for the sign-in", error))?;
    let port = listener
        .local_addr()
        .map_err(|error| unavailable("listen for the sign-in", error))?
        .port();
    let redirect = format!("http://127.0.0.1:{port}/callback");
    let metadata = discover(http, &provider.resource).await?;
    let client = register(http, &metadata, &redirect, provider.scopes).await?;
    let (verifier, state) = (
        random(48).map_err(|error| unavailable("make a sign-in", error))?,
        random(24).map_err(|error| unavailable("make a sign-in", error))?,
    );
    open(&authorize_url(
        &metadata,
        &client.client_id,
        &redirect,
        &verifier,
        &state,
        (&provider.resource, provider.scopes),
    )?);
    let code = wait_for_code(listener, &state).await?;
    let mut form = vec![
        ("grant_type", "authorization_code"),
        ("code", code.as_str()),
        ("redirect_uri", redirect.as_str()),
        ("client_id", client.client_id.as_str()),
        ("code_verifier", verifier.as_str()),
        ("resource", provider.resource.as_str()),
    ];
    if let Some(secret) = &client.client_secret {
        form.push(("client_secret", secret.value()));
    }
    let tokens = tokens(http, &metadata.token_endpoint, &form).await?;
    let session = Session {
        client_id: client.client_id,
        client_secret: client.client_secret.map(Token::into_value),
        token_endpoint: metadata.token_endpoint,
        resource: provider.resource.clone(),
        access_token: tokens.access_token.into_value(),
        refresh_token: tokens.refresh_token.map(Token::into_value),
        expires_at: tokens.expires_in.map(|seconds| now() + seconds),
    };
    keep(secrets, provider.account, &session).await
}

async fn keep(secrets: &dyn Secrets, account: &str, session: &Session) -> Result<(), ServiceError> {
    let text = serde_json::to_string(session)
        .map_err(|error| ServiceError::Unreadable(error.to_string()))?;
    secrets
        .set(account, &Token::new(text))
        .await
        .map_err(|error| unavailable("keep the sign-in in the Keychain", error))
}

/// The session kept under `account`, if the person has signed in.
pub(crate) async fn session(secrets: &dyn Secrets, account: &str) -> Option<Session> {
    let kept = secrets.get(account).await?;
    serde_json::from_str(kept.value()).ok()
}

/// An access token from `session`, refreshed (and the session kept again)
/// if it is near its end.
pub(crate) async fn access_token(
    http: &reqwest::Client,
    secrets: &dyn Secrets,
    account: &str,
    mut session: Session,
) -> Result<Token, ServiceError> {
    if !session.needs_refresh(now()) {
        return Ok(Token::new(session.access_token));
    }
    let Some(refresh) = session.refresh_token.clone() else {
        return Ok(Token::new(session.access_token));
    };
    let mut form = vec![
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh.as_str()),
        ("client_id", session.client_id.as_str()),
        ("resource", session.resource.as_str()),
    ];
    let secret = session.client_secret.clone();
    if let Some(secret) = &secret {
        form.push(("client_secret", secret));
    }
    let tokens = tokens(http, &session.token_endpoint, &form).await?;
    session.access_token = tokens.access_token.into_value();
    if let Some(refresh) = tokens.refresh_token {
        session.refresh_token = Some(refresh.into_value());
    }
    session.expires_at = tokens.expires_in.map(|seconds| now() + seconds);
    keep(secrets, account, &session).await?;
    Ok(Token::new(session.access_token))
}

#[cfg(test)]
mod tests;

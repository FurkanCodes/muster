//! Signing in: PKCE, the callback, refreshing; and a whole sign-in against
//! a stand-in authorization server, the test playing the browser.

use std::sync::{Arc, Mutex};

use axum::Router;
use axum::extract::State;
use axum::routing::{get, post};

use super::*;
use crate::secrets::Kept;

#[test]
fn the_challenge_is_rfc_7636_s() {
    assert_eq!(
        challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
        "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
    );
    let verifier = random(48).unwrap();
    assert!(verifier.len() >= 43 && !verifier.contains(['+', '/', '=']));
}

#[test]
fn metadata_is_looked_for_with_the_path_then_at_the_root() {
    let url = reqwest::Url::parse("https://mcp.atlassian.com/v2/mcp?tools=all").unwrap();
    assert_eq!(
        well_known(&url, "oauth-protected-resource"),
        [
            "https://mcp.atlassian.com/.well-known/oauth-protected-resource/v2/mcp",
            "https://mcp.atlassian.com/.well-known/oauth-protected-resource",
        ]
    );
    let root = reqwest::Url::parse("https://auth.example.com").unwrap();
    assert_eq!(
        well_known(&root, "oauth-authorization-server"),
        ["https://auth.example.com/.well-known/oauth-authorization-server"]
    );
}

#[test]
fn only_the_callback_for_this_sign_in_is_taken() {
    let line = |target: &str| format!("GET {target} HTTP/1.1");
    assert_eq!(
        callback_code(&line("/callback?code=abc&state=s1"), "s1"),
        Some(Ok("abc".to_owned()))
    );
    assert!(matches!(
        callback_code(&line("/callback?code=abc&state=other"), "s1"),
        Some(Err(_))
    ));
    assert!(matches!(
        callback_code(&line("/callback?error=access_denied&state=s1"), "s1"),
        Some(Err(message)) if message.contains("access_denied")
    ));
    assert_eq!(callback_code(&line("/favicon.ico"), "s1"), None);
}

#[test]
fn a_session_is_refreshed_near_its_end_and_never_shows_its_tokens() {
    let session = Session {
        client_id: "c".to_owned(),
        client_secret: None,
        token_endpoint: "https://a/token".to_owned(),
        resource: "https://m/mcp".to_owned(),
        access_token: "access-secret".to_owned(),
        refresh_token: Some("refresh-secret".to_owned()),
        expires_at: Some(1_000),
    };
    assert!(!session.needs_refresh(1_000 - REFRESH_AHEAD_SECONDS - 1));
    assert!(session.needs_refresh(1_000 - REFRESH_AHEAD_SECONDS));
    let shown = format!("{session:?}");
    assert!(!shown.contains("secret"), "{shown}");
    let without = Session {
        refresh_token: None,
        ..session
    };
    assert!(!without.needs_refresh(5_000));
}

/// What the stand-in authorization server saw.
#[derive(Debug, Default)]
struct Seen {
    challenge: Option<String>,
    forms: Vec<Vec<(String, String)>>,
}

type Shared = Arc<Mutex<Seen>>;

async fn token(State(seen): State<Shared>, body: String) -> axum::Json<serde_json::Value> {
    let form: Vec<(String, String)> = reqwest::Url::parse(&format!("http://form/?{body}"))
        .unwrap()
        .query_pairs()
        .into_owned()
        .collect();
    let value = |key: &str| {
        form.iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let mut seen = seen.lock().unwrap();
    seen.forms.push(form.clone());
    let answer = match value("grant_type").as_str() {
        "authorization_code" => {
            assert_eq!(value("code"), "the-code");
            assert_eq!(
                seen.challenge.as_deref(),
                Some(challenge(&value("code_verifier")).as_str())
            );
            serde_json::json!({"access_token": "a1", "refresh_token": "r1", "expires_in": 3600, "token_type": "Bearer"})
        }
        "refresh_token" => {
            assert_eq!(value("refresh_token"), "r1");
            serde_json::json!({"access_token": "a2", "refresh_token": "r2", "expires_in": 3600, "token_type": "Bearer"})
        }
        other => panic!("unexpected grant {other}"),
    };
    axum::Json(answer)
}

#[tokio::test]
async fn a_whole_sign_in_registers_signs_in_keeps_and_refreshes() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let seen: Shared = Arc::default();
    let metadata = serde_json::json!({
        "issuer": base,
        "authorization_endpoint": format!("{base}/authorize"),
        "token_endpoint": format!("{base}/token"),
        "registration_endpoint": format!("{base}/register"),
    });
    let resource_metadata = serde_json::json!({ "authorization_servers": [base] });
    let app = Router::new()
        .route(
            "/.well-known/oauth-protected-resource/mcp",
            get(move || async move { axum::Json(resource_metadata) }),
        )
        .route(
            "/.well-known/oauth-authorization-server",
            get(move || async move { axum::Json(metadata) }),
        )
        .route(
            "/register",
            post(
                |axum::Json(asked): axum::Json<serde_json::Value>| async move {
                    assert_eq!(asked["token_endpoint_auth_method"], "none");
                    assert_eq!(asked["scope"], "read offline_access");
                    assert!(
                        asked["redirect_uris"][0]
                            .as_str()
                            .unwrap()
                            .starts_with("http://127.0.0.1:")
                    );
                    axum::Json(serde_json::json!({"client_id": "c1"}))
                },
            ),
        )
        .route("/token", post(token))
        .with_state(seen.clone());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let http = reqwest::Client::new();
    let secrets = Kept::default();
    let provider = Provider {
        name: "Example",
        resource: format!("{base}/mcp"),
        account: "example:mcp",
        scopes: &["read", "offline_access"],
    };
    // The browser: the person signs in, and is sent back with a code.
    let browser_seen = seen.clone();
    let open = move |address: &str| {
        let url = reqwest::Url::parse(address).unwrap();
        let query: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(query["response_type"], "code");
        assert_eq!(query["client_id"], "c1");
        assert_eq!(query["code_challenge_method"], "S256");
        assert!(query["resource"].ends_with("/mcp"));
        assert_eq!(query["scope"], "read offline_access");
        browser_seen.lock().unwrap().challenge = Some(query["code_challenge"].clone());
        let back = format!(
            "{}?code=the-code&state={}",
            query["redirect_uri"], query["state"]
        );
        tokio::spawn(async move {
            let page = reqwest::get(back).await.unwrap().text().await.unwrap();
            assert!(page.contains("Signed in to Muster"));
        });
    };
    sign_in(&http, &secrets, &provider, open).await.unwrap();
    let kept = session(&secrets, "example:mcp").await.unwrap();
    assert_eq!(
        (kept.client_id.as_str(), kept.access_token.as_str()),
        ("c1", "a1")
    );
    assert_eq!(kept.refresh_token.as_deref(), Some("r1"));

    // Fresh, it is used as it is; near its end, refreshed and kept again.
    let token = access_token(&http, &secrets, "example:mcp", kept.clone())
        .await
        .unwrap();
    assert_eq!(token.value(), "a1");
    let ending = Session {
        expires_at: Some(now()),
        ..kept
    };
    let token = access_token(&http, &secrets, "example:mcp", ending)
        .await
        .unwrap();
    assert_eq!(token.value(), "a2");
    let rotated = session(&secrets, "example:mcp").await.unwrap();
    assert_eq!(rotated.refresh_token.as_deref(), Some("r2"));
    assert_eq!(seen.lock().unwrap().forms.len(), 2);
}

/// Discovery against the real servers, read-only: `cargo test -- --ignored
/// live_discovery`. It needs the internet, so it does not run unasked.
#[tokio::test]
#[ignore = "reaches Atlassian and Linear over the internet"]
async fn live_discovery_finds_where_to_register_and_sign_in() {
    let http = crate::service::http_client();
    for resource in [
        "https://mcp.atlassian.com/v2/mcp",
        "https://mcp.linear.app/mcp",
    ] {
        let found = discover(&http, resource).await.unwrap();
        assert!(
            found.registration_endpoint.is_some(),
            "{resource}: {found:?}"
        );
        println!("{resource}: {found:?}");
    }
}

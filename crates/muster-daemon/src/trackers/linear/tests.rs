//! Linear: issues read in either shape, open ones told apart, the
//! completed state found; and its GraphQL API and MCP server standing in.

use std::sync::{Arc, Mutex as Shared};

use axum::Router;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::post;

use super::*;

#[test]
fn issues_are_read_from_the_api_and_the_mcp_tools() {
    let api = json!({"identifier": "ENG-42", "title": "Fix the login redirect", "url": "https://linear.app/acme/issue/ENG-42",
        "description": "Users land on /404.\u{1b}[2J", "labels": {"nodes": [{"name": "bug"}]}, "creator": {"name": "Some One"}});
    let detail = read_issue(&api).unwrap();
    assert_eq!(detail.issue.key.as_str(), "ENG-42");
    assert_eq!(detail.issue.labels, ["bug"]);
    assert_eq!(detail.issue.author.as_deref(), Some("Some One"));
    assert_eq!(detail.body, "Users land on /404.[2J");
    let mcp = json!({"id": "ENG-7", "title": "Dark mode", "labels": ["ui"], "status": "In Progress", "createdBy": "Other"});
    let detail = read_issue(&mcp).unwrap();
    assert_eq!(
        (detail.issue.key.as_str(), detail.issue.labels.clone()),
        ("ENG-7", vec!["ui".to_owned()])
    );
    assert!(read_issue(&json!({"id": "9b6f-uuid", "title": "x"})).is_none());
}

#[test]
fn open_issues_are_those_not_completed_or_canceled() {
    assert!(is_open(
        &json!({"state": {"type": "started", "name": "In Progress"}})
    ));
    assert!(!is_open(
        &json!({"state": {"type": "completed", "name": "Shipped"}})
    ));
    assert!(!is_open(&json!({"status": "Done"})));
    assert!(!is_open(&json!({"status": "Canceled"})));
    assert!(is_open(&json!({"status": "Todo"})));
    let states = [
        json!({"id": "s3", "name": "Released", "type": "completed", "position": 3.0}),
        json!({"id": "s1", "name": "Todo", "type": "unstarted", "position": 1.0}),
        json!({"id": "s2", "name": "Done", "type": "completed", "position": 2.0}),
    ];
    assert_eq!(
        completed(&states),
        Some(("s2".to_owned(), "Done".to_owned()))
    );
}

#[derive(Debug, Default)]
struct Seen {
    asked: Vec<Value>,
}

type Recorded = Arc<Shared<Seen>>;

async fn api(
    State(seen): State<Recorded>,
    headers: HeaderMap,
    axum::Json(body): axum::Json<Value>,
) -> Response {
    // A personal key goes as it is, not as a bearer token.
    assert_eq!(
        headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok()),
        Some("lin_api_k")
    );
    seen.lock().unwrap().asked.push(body.clone());
    let query = body["query"].as_str().unwrap_or_default();
    let data = if query.contains("issues(") {
        assert_eq!(body["variables"]["team"], "ENG");

        json!({"issues": {"nodes": [
            {"identifier": "ENG-1", "title": "Old", "url": "u1", "createdAt": "2026-01-01T00:00:00Z"},
            {"identifier": "ENG-2", "title": "New", "url": "u2", "createdAt": "2026-10-01T00:00:00Z"}
        ]}})
    } else if query.contains("issueUpdate") {
        assert_eq!(body["variables"]["state"], "s-done");
        json!({"issueUpdate": {"success": true}})
    } else if query.contains("states") {
        json!({"issue": {"team": {"states": {"nodes": [{"id": "s-done", "name": "Done", "type": "completed", "position": 1}]}}}})
    } else if body["variables"]["id"] == "ENG-2" {
        json!({"issue": {"identifier": "ENG-2", "title": "New", "url": "u2", "description": "text"}})
    } else {
        return axum::Json(json!({"errors": [{"message": "Entity not found: Issue", "extensions": {"code": "ENTITY_NOT_FOUND"}}]})).into_response();
    };
    axum::Json(json!({ "data": data })).into_response()
}

async fn serve(app: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    base
}

#[tokio::test]
async fn the_api_lists_newest_first_views_and_closes_with_the_key() {
    let seen: Recorded = Arc::default();
    let base = serve(
        Router::new()
            .route("/graphql", post(api))
            .with_state(seen.clone()),
    )
    .await;
    let linear = Linear {
        team: "ENG".to_owned(),
        mine: false,
        access: Access::Api {
            http: reqwest::Client::new(),
            url: format!("{base}/graphql"),
            key: Token::new("lin_api_k"),
        },
    };
    let listed = linear.list().await.unwrap();
    let keys: Vec<&str> = listed.iter().map(|issue| issue.key.as_str()).collect();
    assert_eq!(keys, ["ENG-2", "ENG-1"]);
    let key = TicketKey::parse("ENG-2").unwrap();
    assert_eq!(linear.view(&key).await.unwrap().body, "text");
    let missing = linear
        .view(&TicketKey::parse("ENG-9").unwrap())
        .await
        .unwrap_err();
    assert!(matches!(missing, ServiceError::NotFound(_)), "{missing}");
    linear.close(&key).await.unwrap();
    assert_eq!(linear.describe(), "Linear (ENG)");
}

async fn mcp_server(
    State(seen): State<Recorded>,
    axum::Json(message): axum::Json<Value>,
) -> Response {
    let id = message["id"].clone();
    let reply = |result: Value| {
        axum::Json(json!({"jsonrpc": "2.0", "id": id, "result": result})).into_response()
    };
    let text = |value: Value| json!({"content": [{"type": "text", "text": value.to_string()}]});
    match message["method"].as_str().unwrap_or_default() {
        "initialize" => reply(json!({"protocolVersion": "2025-06-18", "capabilities": {}})),
        "notifications/initialized" => StatusCode::ACCEPTED.into_response(),
        "tools/list" => reply(json!({"tools": [
            {"name": "list_issues", "inputSchema": {"properties": {"team": {}, "limit": {}, "assignee": {}}}},
            {"name": "get_issue", "inputSchema": {"properties": {"id": {}}}},
            {"name": "list_issue_statuses", "inputSchema": {"properties": {"team": {}}}},
            {"name": "update_issue", "inputSchema": {"properties": {"id": {}, "state": {}}}}
        ]})),
        "tools/call" => {
            let (name, args) = (
                message["params"]["name"].as_str().unwrap_or_default(),
                message["params"]["arguments"].clone(),
            );
            seen.lock()
                .unwrap()
                .asked
                .push(json!({"tool": name, "args": args}));
            reply(match name {
                "list_issues" => text(json!([
                    {"id": "ENG-3", "title": "Open one", "status": "In Progress", "createdAt": "2026-10-02T00:00:00Z"},
                    {"id": "ENG-4", "title": "Done one", "status": "Done", "createdAt": "2026-10-03T00:00:00Z"}
                ])),
                "get_issue" => {
                    text(json!({"id": "ENG-3", "title": "Open one", "description": "Its text."}))
                }
                "list_issue_statuses" => {
                    text(json!([{"id": "st-1", "name": "Done", "type": "completed"}]))
                }
                "update_issue" => text(json!({"success": true})),
                _ => json!({"content": [], "isError": true}),
            })
        }
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

#[tokio::test]
async fn mine_asks_linear_for_the_person_s_own() {
    let seen: Recorded = Arc::default();
    let base = serve(
        Router::new()
            .route("/graphql", post(api))
            .with_state(seen.clone()),
    )
    .await;
    let linear = Linear {
        team: "ENG".to_owned(),
        mine: true,
        access: Access::Api {
            http: reqwest::Client::new(),
            url: format!("{base}/graphql"),
            key: Token::new("lin_api_k"),
        },
    };
    linear.list().await.unwrap();
    let query = seen.lock().unwrap().asked[0]["query"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        query.contains("assignee: { isMe: { eq: true } }"),
        "{query}"
    );

    let mcp_seen: Recorded = Arc::default();
    let base = serve(
        Router::new()
            .route("/mcp", post(mcp_server))
            .with_state(mcp_seen.clone()),
    )
    .await;
    let mcp = Mcp::connect(
        reqwest::Client::new(),
        &format!("{base}/mcp"),
        Token::new("a"),
        "Linear",
        "muster auth linear",
    )
    .await
    .unwrap();
    let linear = Linear {
        team: "ENG".to_owned(),
        mine: true,
        access: Access::Mcp {
            mcp: Mutex::new(mcp),
        },
    };
    linear.list().await.unwrap();
    let asked = mcp_seen.lock().unwrap().asked[0]["args"].clone();
    assert_eq!(asked["assignee"], "me");
}

#[tokio::test]
async fn linear_s_mcp_server_lists_open_issues_views_and_closes_by_state_name() {
    let seen: Recorded = Arc::default();
    let base = serve(
        Router::new()
            .route("/mcp", post(mcp_server))
            .with_state(seen.clone()),
    )
    .await;
    let mcp = Mcp::connect(
        reqwest::Client::new(),
        &format!("{base}/mcp"),
        Token::new("a"),
        "Linear",
        "muster auth linear",
    )
    .await
    .unwrap();
    let linear = Linear {
        team: "ENG".to_owned(),
        mine: false,
        access: Access::Mcp {
            mcp: Mutex::new(mcp),
        },
    };
    let listed = linear.list().await.unwrap();
    assert_eq!(
        listed
            .iter()
            .map(|issue| issue.key.as_str())
            .collect::<Vec<_>>(),
        ["ENG-3"]
    );
    let key = TicketKey::parse("ENG-3").unwrap();
    assert_eq!(linear.view(&key).await.unwrap().body, "Its text.");
    linear.close(&key).await.unwrap();
    let asked = seen.lock().unwrap().asked.clone();
    assert_eq!(
        asked[0]["args"],
        json!({"team": "ENG", "limit": 100}),
        "not only the person's"
    );
    assert_eq!(
        asked.last().unwrap()["args"],
        json!({"id": "ENG-3", "state": "Done"})
    );
}

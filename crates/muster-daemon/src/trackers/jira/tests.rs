//! Jira: sites, descriptions, transitions; and listing, viewing and
//! closing through stand-ins for its REST API and Atlassian's MCP server.

use std::sync::{Arc, Mutex as Shared};

use axum::Router;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};

use super::*;

#[test]
fn sites_are_read_as_written() {
    let cloud = Site::parse("acme.atlassian.net").unwrap();
    assert_eq!(
        (cloud.web.as_str(), cloud.host.as_str()),
        ("https://acme.atlassian.net", "acme.atlassian.net")
    );
    assert!(cloud.is_cloud());
    let data_center = Site::parse("https://jira.example.com/").unwrap();
    assert_eq!(data_center.web, "https://jira.example.com");
    assert!(!data_center.is_cloud());
    assert_eq!(Site::parse("not a site"), None);
}

#[test]
fn a_description_in_atlassian_s_format_reads_as_text() {
    let adf = json!({"type": "doc", "version": 1, "content": [
        {"type": "paragraph", "content": [{"type": "text", "text": "After signing in,"}, {"type": "hardBreak"}, {"type": "text", "text": "users land on /404."}]},
        {"type": "bulletList", "content": [{"type": "listItem", "content": [{"type": "paragraph", "content": [{"type": "text", "text": "Chrome"}]}]}]}
    ]});
    assert_eq!(
        description_text(&adf),
        "After signing in,\nusers land on /404.\nChrome"
    );
    assert_eq!(description_text(&json!("wiki *text*")), "wiki *text*");
    assert_eq!(description_text(&Value::Null), "");
}

#[test]
fn the_done_transition_is_found_by_its_category() {
    let transitions = json!({"transitions": [
        {"id": "11", "name": "To Do", "to": {"statusCategory": {"key": "new"}}},
        {"id": 31, "name": "Shipped", "to": {"statusCategory": {"key": "done"}}}
    ]});
    assert_eq!(done_transition(&transitions).as_deref(), Some("31"));
    assert_eq!(done_transition(&json!({"transitions": []})), None);
    assert_eq!(
        project_query("APP"),
        "project = \"APP\" AND statusCategory != Done ORDER BY created DESC"
    );
}

fn item(key: &str, summary: &str) -> Value {
    json!({"id": "10001", "key": key, "fields": {
        "summary": summary, "labels": ["bug"], "reporter": {"displayName": "Some One"},
        "description": {"type": "doc", "content": [{"type": "paragraph", "content": [{"type": "text", "text": "Users land on /404.\u{1b}[2J"}]}]}
    }})
}

#[derive(Debug, Default)]
struct Seen {
    asked: Vec<String>,
    moved: Vec<Value>,
}

type Recorded = Arc<Shared<Seen>>;

async fn rest_search(
    State(seen): State<Recorded>,
    headers: HeaderMap,
    axum::Json(body): axum::Json<Value>,
) -> Response {
    // email:token, as Basic auth.
    assert_eq!(
        headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok()),
        Some("Basic bWVAZXhhbXBsZS5jb206dDBrZW4=")
    );
    seen.lock()
        .unwrap()
        .asked
        .push(format!("search {}", body["jql"]));
    axum::Json(json!({"issues": [item("APP-7", "Fix the login redirect")], "isLast": true}))
        .into_response()
}

async fn rest_issue(Path(key): Path<String>) -> Response {
    if key == "APP-7" {
        axum::Json(item("APP-7", "Fix the login redirect")).into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            axum::Json(json!({"errorMessages": ["Issue does not exist"]})),
        )
            .into_response()
    }
}

async fn rest_transitions() -> axum::Json<Value> {
    axum::Json(json!({"transitions": [{"id": "31", "to": {"statusCategory": {"key": "done"}}}]}))
}

async fn rest_move(
    State(seen): State<Recorded>,
    axum::Json(body): axum::Json<Value>,
) -> StatusCode {
    seen.lock().unwrap().moved.push(body);
    StatusCode::NO_CONTENT
}

async fn serve(app: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    base
}

#[tokio::test]
async fn jira_cloud_s_rest_api_lists_views_and_closes_with_a_token() {
    let seen: Recorded = Arc::default();
    let app = Router::new()
        .route("/rest/api/3/search/jql", post(rest_search))
        .route("/rest/api/3/issue/{key}", get(rest_issue))
        .route(
            "/rest/api/3/issue/{key}/transitions",
            get(rest_transitions).post(rest_move),
        )
        .with_state(seen.clone());
    let base = serve(app).await;
    let jira = Jira {
        site: Site::parse("acme.atlassian.net").unwrap(),
        jql: project_query("APP"),
        access: Access::Rest {
            http: reqwest::Client::new(),
            api: format!("{base}/rest/api/3"),
            email: Some("me@example.com".to_owned()),
            token: Token::new("t0ken"),
        },
    };
    let listed = jira.list().await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].key.as_str(), "APP-7");
    assert_eq!(listed[0].url, "https://acme.atlassian.net/browse/APP-7");
    assert_eq!(listed[0].author.as_deref(), Some("Some One"));
    assert_eq!(
        seen.lock().unwrap().asked,
        [format!(
            "search \"{}\"",
            project_query("APP").replace('"', "\\\"")
        )]
    );

    let key = TicketKey::parse("APP-7").unwrap();
    let detail = jira.view(&key).await.unwrap();
    assert_eq!(
        detail.body, "Users land on /404.[2J",
        "control characters go"
    );
    assert_eq!(detail.tracker, TrackerKind::Jira);
    let missing = jira
        .view(&TicketKey::parse("APP-9").unwrap())
        .await
        .unwrap_err();
    assert!(matches!(missing, ServiceError::NotFound(_)), "{missing}");

    jira.close(&key).await.unwrap();
    assert_eq!(
        seen.lock().unwrap().moved,
        [json!({"transition": {"id": "31"}})]
    );
    assert_eq!(jira.describe(), "Jira (acme.atlassian.net)");
}

/// Atlassian's MCP server, standing in: the tools Muster calls, answering
/// as text holding JSON.
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
            {"name": "getAccessibleAtlassianResources", "inputSchema": {"properties": {}}},
            {"name": "searchJiraIssuesUsingJql", "inputSchema": {"properties": {"cloudId": {}, "jql": {}, "fields": {}, "maxResults": {}}}},
            {"name": "getJiraIssue", "inputSchema": {"properties": {"cloudId": {}, "issueIdOrKey": {}}}},
            {"name": "listJiraIssueTransitions", "inputSchema": {"properties": {"cloudId": {}, "issueIdOrKey": {}}}},
            {"name": "transitionJiraIssue", "inputSchema": {"properties": {"cloudId": {}, "issueIdOrKey": {}, "transition": {}}}}
        ]})),
        "tools/call" => {
            let (name, args) = (
                message["params"]["name"].as_str().unwrap_or_default(),
                &message["params"]["arguments"],
            );
            seen.lock().unwrap().asked.push(format!("{name} {args}"));
            if name != "getAccessibleAtlassianResources" {
                assert_eq!(args["cloudId"], "cloud-1", "{name}");
            }
            reply(match name {
                "getAccessibleAtlassianResources" => text(json!([
                    {"id": "cloud-0", "url": "https://other.atlassian.net", "name": "other"},
                    {"id": "cloud-1", "url": "https://acme.atlassian.net", "name": "acme"}
                ])),
                "searchJiraIssuesUsingJql" => {
                    text(json!({"issues": [item("APP-7", "Fix the login redirect")]}))
                }
                "getJiraIssue" => text(item("APP-7", "Fix the login redirect")),
                "listJiraIssueTransitions" => text(
                    json!({"transitions": [{"id": "41", "to": {"statusCategory": {"key": "done"}}}]}),
                ),
                "transitionJiraIssue" => {
                    seen.lock().unwrap().moved.push(args["transition"].clone());
                    text(json!({"success": true}))
                }
                _ => json!({"content": [{"type": "text", "text": "unknown"}], "isError": true}),
            })
        }
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

#[tokio::test]
async fn atlassian_s_mcp_server_lists_views_and_closes_on_the_site_s_cloud() {
    let seen: Recorded = Arc::default();
    let base = serve(
        Router::new()
            .route("/v2/mcp", post(mcp_server))
            .with_state(seen.clone()),
    )
    .await;
    let mcp = Mcp::connect(
        reqwest::Client::new(),
        &format!("{base}/v2/mcp"),
        Token::new("a"),
        "Atlassian",
        "muster auth jira",
    )
    .await
    .unwrap();
    let jira = Jira {
        site: Site::parse("acme.atlassian.net").unwrap(),
        jql: "assignee = currentUser()".to_owned(),
        access: Access::Mcp {
            mcp: Mutex::new(mcp),
        },
    };
    let listed = jira.list().await.unwrap();
    assert_eq!(listed[0].key.as_str(), "APP-7");
    let key = TicketKey::parse("APP-7").unwrap();
    assert_eq!(jira.view(&key).await.unwrap().issue.labels, ["bug"]);
    jira.close(&key).await.unwrap();
    assert_eq!(seen.lock().unwrap().moved, [json!({"id": "41"})]);
    let asked = seen.lock().unwrap().asked.join("\n");
    assert!(
        asked.contains("\"jql\":\"assignee = currentUser()\""),
        "{asked}"
    );
    assert!(asked.contains("\"issueIdOrKey\":\"APP-7\""), "{asked}");
}

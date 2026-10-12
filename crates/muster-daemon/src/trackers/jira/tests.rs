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
fn mine_narrows_any_query_to_the_person_s_and_keeps_its_order() {
    assert_eq!(
        assigned_to_me("project = APP ORDER BY created DESC"),
        "(project = APP) AND assignee = currentUser() ORDER BY created DESC"
    );
    assert_eq!(
        assigned_to_me("labels = ui OR labels = ux"),
        "(labels = ui OR labels = ux) AND assignee = currentUser()"
    );
    assert_eq!(
        assigned_to_me("order by rank"),
        "assignee = currentUser() order by rank"
    );
    let project = Tickets {
        project: Some("APP".to_owned()),
        mine: true,
        ..Tickets::default()
    };
    assert_eq!(
        query(&project).unwrap(),
        "(project = \"APP\" AND statusCategory != Done) AND assignee = currentUser() ORDER BY created DESC"
    );
    let all = Tickets {
        mine: false,
        ..project
    };
    assert_eq!(query(&all).unwrap(), project_query("APP"));
    assert_eq!(query(&Tickets::default()), None);
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
        mine: false,
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
            {"name": "searchJiraIssuesUsingJql", "inputSchema": {"properties": {"cloudId": {}, "jql": {}, "fields": {}, "maxResults": {}, "view": {}}}},
            {"name": "getJiraIssue", "inputSchema": {"properties": {"cloudId": {}, "issueIdOrKey": {}, "fields": {}, "view": {}, "responseContentFormat": {}}}},
            {"name": "listJiraIssueTransitions", "inputSchema": {"properties": {"cloudId": {}, "issueIdOrKey": {}}}},
            {"name": "transitionJiraIssue", "inputSchema": {"properties": {"cloudId": {}, "issueIdOrKey": {}, "transitionId": {}, "transitionName": {}}}}
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
                // As Atlassian's v2 answers (seen 2026-10-12): wrapped in `data`.
                "getAccessibleAtlassianResources" => text(json!({"data": {"resources": [
                    {"cloudId": "cloud-0", "url": "https://other.atlassian.net", "products": [{"id": "jira"}]},
                    {"cloudId": "cloud-1", "url": "https://acme.atlassian.net", "products": [{"id": "jira"}]}
                ]}})),
                "searchJiraIssuesUsingJql" => {
                    assert_eq!(args["view"], "evidence");
                    text(
                        json!({"data": {"isLast": true, "issues": [item("APP-7", "Fix the login redirect")]}}),
                    )
                }
                "getJiraIssue" => {
                    assert!(args.get("fields").is_none(), "its fields are custom ones");
                    let mut found = item("APP-7", "Fix the login redirect");
                    found["fields"]["description"] = json!("Users land on /404.\u{1b}[2J");
                    text(json!({"data": found}))
                }
                "listJiraIssueTransitions" => text(json!({"data": {"transitions": [
                    {"id": "40", "isAvailable": false, "to": {"statusCategory": {"key": "done"}}},
                    {"id": "41", "isAvailable": true, "to": {"statusCategory": {"key": "done"}}}
                ]}})),
                "transitionJiraIssue" => {
                    seen.lock()
                        .unwrap()
                        .moved
                        .push(args["transitionId"].clone());
                    text(json!({"data": {"success": true}}))
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
        mine: false,
        access: Access::Mcp {
            mcp: Mutex::new(mcp),
        },
    };
    let listed = jira.list().await.unwrap();
    assert_eq!(listed[0].key.as_str(), "APP-7");
    let key = TicketKey::parse("APP-7").unwrap();
    let viewed = jira.view(&key).await.unwrap();
    assert_eq!(viewed.issue.labels, ["bug"]);
    assert_eq!(
        viewed.body, "Users land on /404.[2J",
        "Markdown, as it is, cleaned"
    );
    jira.close(&key).await.unwrap();
    // The available transition into Done, by its id.
    assert_eq!(seen.lock().unwrap().moved, [json!("41")]);
    let asked = seen.lock().unwrap().asked.join("\n");
    assert!(
        asked.contains("\"jql\":\"assignee = currentUser()\""),
        "{asked}"
    );
    assert!(asked.contains("\"issueIdOrKey\":\"APP-7\""), "{asked}");
}

/// What Atlassian's MCP server offers the person's own sign-in, read-only:
/// `cargo test -- --ignored live_atlassian`. It reads the sign-in kept by
/// `muster auth jira` and prints the sites it reaches and the tools named.
#[tokio::test]
#[ignore = "reads the person's Atlassian sign-in and reaches Atlassian"]
async fn live_atlassian_sites_and_tools() {
    let secrets = crate::secrets::Keychain::default();
    let http = crate::service::http_client();
    let session = oauth::session(&secrets, MCP_ACCOUNT)
        .await
        .expect("signed in with `muster auth jira`");
    let token = oauth::access_token(&http, &secrets, MCP_ACCOUNT, session)
        .await
        .unwrap();
    let mut mcp = Mcp::connect(http, MCP_URL, token, "Atlassian", "muster auth jira")
        .await
        .unwrap();
    let wanted = [
        "getAccessibleAtlassianResources",
        "searchJiraIssuesUsingJql",
        "getJiraIssue",
        "listJiraIssueTransitions",
        "getTransitionsForJiraIssue",
        "transitionJiraIssue",
    ];
    for name in wanted {
        let schema = mcp.tools.iter().find(|tool| tool.name == name).map(|tool| {
            tool.input_schema
                .get("properties")
                .map(|p| p.as_object().map(|o| o.keys().cloned().collect::<Vec<_>>()))
        });
        println!("{name}: {schema:?}");
    }
    println!("{} tools in all", mcp.tools.len());
    for name in [
        "searchJiraIssuesUsingJql",
        "getJiraIssue",
        "transitionJiraIssue",
        "listJiraIssueTransitions",
    ] {
        if let Some(tool) = mcp.tools.iter().find(|tool| tool.name == name) {
            for key in [
                "responseContentFormat",
                "view",
                "searchResultMode",
                "fields",
                "transitionId",
                "maxResults",
                "cloudId",
            ] {
                if let Some(property) = tool.input_schema.pointer(&format!("/properties/{key}")) {
                    println!(
                        "{name}.{key}: {}",
                        property.to_string().chars().take(400).collect::<String>()
                    );
                }
            }
            println!(
                "{name} required: {}",
                tool.input_schema
                    .get("required")
                    .cloned()
                    .unwrap_or_default()
            );
        }
    }
    let tool = mcp.tool(&["getAccessibleAtlassianResources"]).unwrap();
    let resources = mcp.call(&tool, json!({})).await.unwrap();
    // The shape of one search result, one ticket and its transitions, never
    // their text: keys only, and the transitions' categories.
    fn shape(value: &Value, depth: usize) -> String {
        match value {
            Value::Object(map) if depth > 0 => format!(
                "{{{}}}",
                map.iter()
                    .map(|(key, value)| format!("{key}: {}", shape(value, depth - 1)))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Value::Array(items) if depth > 0 => format!(
                "[{} × {}]",
                items.len(),
                items
                    .first()
                    .map_or("-".to_owned(), |item| shape(item, depth - 1))
            ),
            Value::String(_) => "str".to_owned(),
            Value::Number(_) => "num".to_owned(),
            Value::Bool(_) => "bool".to_owned(),
            Value::Null => "null".to_owned(),
            _ => "…".to_owned(),
        }
    }
    let cloud = resources
        .pointer("/data/resources/0/cloudId")
        .cloned()
        .unwrap_or_default();
    let search = mcp.tool(&["searchJiraIssuesUsingJql"]).unwrap();
    let found = mcp
        .call(&search, json!({"cloudId": cloud, "jql": "created >= -365d ORDER BY created DESC", "maxResults": 1, "fields": ["summary", "labels", "reporter", "status"]}))
        .await
        .unwrap();
    println!("search: {}", shape(&found, 6));
    let key = found
        .pointer("/data/issues/0/key")
        .or_else(|| found.pointer("/issues/0/key"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    if let Some(key) = key {
        let get = mcp.tool(&["getJiraIssue"]).unwrap();
        let issue = mcp
            .call(
                &get,
                json!({"cloudId": cloud, "issueIdOrKey": key, "view": "evidence"}),
            )
            .await
            .unwrap();
        println!("issue: {}", shape(&issue, 6));
        let list = mcp.tool(&["listJiraIssueTransitions"]).unwrap();
        let transitions = mcp
            .call(&list, json!({"cloudId": cloud, "issueIdOrKey": key}))
            .await
            .unwrap();
        println!("transitions: {}", shape(&transitions, 7));
        let categories: Vec<String> = transitions
            .pointer("/data/transitions")
            .or_else(|| transitions.get("transitions"))
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .map(|t| {
                        format!(
                            "{} -> {}",
                            t["id"],
                            t.pointer("/to/statusCategory/key")
                                .cloned()
                                .unwrap_or_default()
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        println!("transition categories: {categories:?}");
    }
    let list = resources.as_array().cloned().or_else(|| {
        resources
            .get("resources")
            .and_then(Value::as_array)
            .cloned()
    });
    match list {
        Some(list) => {
            for resource in list {
                println!("site: {} {}", resource["url"], resource["name"]);
            }
        }
        None => println!(
            "resources answered in another shape: {}",
            resources.to_string().chars().take(300).collect::<String>()
        ),
    }
}

/// Muster's own listing and viewing against the person's real site,
/// read-only: `cargo test -- --ignored live_jira_lists`. It prints counts
/// and what came through, never ticket text, and closes nothing.
#[tokio::test]
#[ignore = "reads the person's Jira through their Atlassian sign-in"]
async fn live_jira_lists_and_views_through_muster_s_own_code() {
    let secrets = crate::secrets::Keychain::default();
    let http = crate::service::http_client();
    let session = oauth::session(&secrets, MCP_ACCOUNT)
        .await
        .expect("signed in");
    let token = oauth::access_token(&http, &secrets, MCP_ACCOUNT, session)
        .await
        .unwrap();
    let mut mcp = Mcp::connect(http, MCP_URL, token, "Atlassian", "muster auth jira")
        .await
        .unwrap();
    let tool = mcp.tool(&["getAccessibleAtlassianResources"]).unwrap();
    let resources = mcp.call(&tool, json!({})).await.unwrap();
    let site = resources
        .pointer("/data/resources/0/url")
        .and_then(Value::as_str)
        .unwrap()
        .to_owned();
    let jira = Jira {
        site: Site::parse(&site).unwrap(),
        jql: "created >= -90d ORDER BY created DESC".to_owned(),
        mine: false,
        access: Access::Mcp {
            mcp: Mutex::new(mcp),
        },
    };
    let listed = jira.list().await.unwrap();
    println!("listed: {} tickets", listed.len());
    let titled = listed
        .iter()
        .filter(|issue| !issue.title.is_empty())
        .count();
    let linked = listed
        .iter()
        .filter(|issue| issue.url.contains("/browse/"))
        .count();
    let authored = listed.iter().filter(|issue| issue.author.is_some()).count();
    println!("with titles: {titled}, links: {linked}, reporters: {authored}");
    if let Some(first) = listed.first() {
        let viewed = jira.view(&first.key).await.unwrap();
        println!(
            "viewed one: title {}, text {} characters, labels {}, by {}",
            !viewed.issue.title.is_empty(),
            viewed.body.chars().count(),
            viewed.issue.labels.len(),
            viewed.issue.author.is_some()
        );
    }
}

/// The person's Jira projects, keys and names only, read-only: `cargo
/// test -- --ignored live_jira_projects`.
#[tokio::test]
#[ignore = "reads the person's Jira through their Atlassian sign-in"]
async fn live_jira_projects() {
    let secrets = crate::secrets::Keychain::default();
    let http = crate::service::http_client();
    let session = oauth::session(&secrets, MCP_ACCOUNT)
        .await
        .expect("signed in");
    let token = oauth::access_token(&http, &secrets, MCP_ACCOUNT, session)
        .await
        .unwrap();
    let mut mcp = Mcp::connect(http, MCP_URL, token, "Atlassian", "muster auth jira")
        .await
        .unwrap();
    let tool = mcp.tool(&["getAccessibleAtlassianResources"]).unwrap();
    let resources = mcp.call(&tool, json!({})).await.unwrap();
    let cloud = resources
        .pointer("/data/resources/0/cloudId")
        .cloned()
        .unwrap_or_default();
    let list = mcp.tool(&["listJiraProjects"]).unwrap();
    let args = list.args(&[
        (&["cloudId"], cloud),
        (&["maxResults", "limit"], json!(100)),
    ]);
    let found = mcp.call(&list, args).await.unwrap();
    let found = unwrapped(&found);
    let projects = found
        .get("projects")
        .or_else(|| found.get("values"))
        .and_then(Value::as_array)
        .cloned()
        .or_else(|| found.as_array().cloned())
        .unwrap_or_default();
    for project in projects {
        println!("project: {} {}", project["key"], project["name"]);
    }
}

/// How many tickets a query of the person's lists, read-only: `MUSTER_JQL=…
/// cargo test -- --ignored live_jira_query`. It prints the count and keys.
#[tokio::test]
#[ignore = "reads the person's Jira through their Atlassian sign-in"]
async fn live_jira_query() {
    let jql = std::env::var("MUSTER_JQL").expect("MUSTER_JQL");
    let secrets = crate::secrets::Keychain::default();
    let http = crate::service::http_client();
    let session = oauth::session(&secrets, MCP_ACCOUNT)
        .await
        .expect("signed in");
    let token = oauth::access_token(&http, &secrets, MCP_ACCOUNT, session)
        .await
        .unwrap();
    let mcp = Mcp::connect(http, MCP_URL, token, "Atlassian", "muster auth jira")
        .await
        .unwrap();
    let jira = Jira {
        site: Site::parse("acme.atlassian.net").unwrap(),
        jql,
        mine: false,
        access: Access::Mcp {
            mcp: Mutex::new(mcp),
        },
    };
    let listed = jira.list().await.unwrap();
    let keys: Vec<&str> = listed.iter().map(|issue| issue.key.as_str()).collect();
    println!("listed: {} {keys:?}", listed.len());
}

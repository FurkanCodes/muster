//! The MCP client against a stand-in server: the handshake, the session
//! carried on, tools listed, and calls answered as JSON or as a stream.

use axum::Router;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::post;

use super::*;

async fn server(headers: HeaderMap, axum::Json(message): axum::Json<Value>) -> Response {
    assert_eq!(
        headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok()),
        Some("Bearer t0ken")
    );
    let method = message["method"].as_str().unwrap_or_default();
    let id = message["id"].clone();
    if method != "initialize" {
        // The session the server named is carried on.
        if headers.get("mcp-session-id").and_then(|v| v.to_str().ok()) != Some("s-1") {
            return StatusCode::BAD_REQUEST.into_response();
        }
    }
    match method {
        "initialize" => (
            [("Mcp-Session-Id", "s-1")],
            axum::Json(json!({"jsonrpc": "2.0", "id": id, "result": {
                "protocolVersion": "2025-03-26", "capabilities": {"tools": {}},
                "serverInfo": {"name": "stand-in", "version": "1"}}})),
        )
            .into_response(),
        "notifications/initialized" => StatusCode::ACCEPTED.into_response(),
        "tools/list" => {
            let page = if message["params"]["cursor"].is_null() {
                json!({"tools": [{"name": "searchJiraIssuesUsingJql", "inputSchema": {"type": "object",
                    "properties": {"cloudId": {}, "jql": {}, "maxResults": {}}}}], "nextCursor": "2"})
            } else {
                json!({"tools": [{"name": "getJiraIssue", "inputSchema": {"type": "object",
                    "properties": {"cloudId": {}, "issueIdOrKey": {}}}}]})
            };
            axum::Json(json!({"jsonrpc": "2.0", "id": id, "result": page})).into_response()
        }
        "tools/call" => {
            let name = message["params"]["name"].as_str().unwrap_or_default();
            let result = match name {
                "searchJiraIssuesUsingJql" => json!({"content": [{"type": "text",
                    "text": json!({"issues": [{"key": "APP-7"}], "args": message["params"]["arguments"]}).to_string()}]}),
                _ => {
                    json!({"content": [{"type": "text", "text": "No issue APP-9"}], "isError": true})
                }
            };
            // A stream: a notification first, then the answer.
            let stream = format!(
                "event: message\ndata: {}\n\nevent: message\ndata: {}\n\n",
                json!({"jsonrpc": "2.0", "method": "notifications/progress", "params": {}}),
                json!({"jsonrpc": "2.0", "id": id, "result": result})
            );
            ([(header::CONTENT_TYPE, "text/event-stream")], stream).into_response()
        }
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

#[tokio::test]
async fn tools_are_listed_and_called_through_a_session() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/mcp", listener.local_addr().unwrap());
    #[expect(
        clippy::disallowed_methods,
        reason = "a test's task ends with the test"
    )]
    tokio::spawn(async move {
        axum::serve(listener, Router::new().route("/mcp", post(server)))
            .await
            .unwrap();
    });
    let mut mcp = Mcp::connect(
        reqwest::Client::new(),
        &url,
        Token::new("t0ken"),
        "Example",
        "muster auth x",
    )
    .await
    .unwrap();
    assert_eq!(mcp.protocol, "2025-03-26");
    let names: Vec<&str> = mcp.tools.iter().map(|tool| tool.name.as_str()).collect();
    assert_eq!(names, ["searchJiraIssuesUsingJql", "getJiraIssue"]);

    let search = mcp.tool(&["searchJiraIssuesUsingJql"]).unwrap();
    let args = search.args(&[
        (&["cloudId"], json!("c-1")),
        (&["jql"], json!("project = APP")),
        (&["limit", "maxResults"], json!(50)),
    ]);
    assert_eq!(
        args,
        json!({"cloudId": "c-1", "jql": "project = APP", "maxResults": 50})
    );
    let found = mcp.call(&search, args).await.unwrap();
    assert_eq!(found["issues"][0]["key"], "APP-7");
    assert_eq!(found["args"]["maxResults"], 50);

    let get = mcp.tool(&["getIssue", "getJiraIssue"]).unwrap();
    let refused = mcp.call(&get, json!({})).await.unwrap_err();
    assert!(refused.to_string().contains("No issue APP-9"), "{refused}");
    assert!(mcp.tool(&["deleteEverything"]).is_err());
}

#[test]
fn the_answer_is_found_among_a_stream_s_events() {
    let stream = "event: message\ndata: {\"jsonrpc\":\"2.0\",\"method\":\"x\"}\n\n: comment\ndata: {\"jsonrpc\":\"2.0\",\ndata: \"id\":3,\"result\":{\"ok\":true}}\n\n";
    assert_eq!(
        answer_in_stream(stream, 3),
        Some(json!({"jsonrpc": "2.0", "id": 3, "result": {"ok": true}}))
    );
    assert_eq!(answer_in_stream(stream, 4), None);
}

#[test]
fn arguments_go_under_the_names_a_tool_takes() {
    let tool = Tool {
        name: "list_issues".to_owned(),
        input_schema: json!({"properties": {"team": {}, "limit": {}}}),
    };
    assert_eq!(
        tool.args(&[
            (&["teamId", "team"], json!("ENG")),
            (&["first", "limit"], json!(100))
        ]),
        json!({"team": "ENG", "limit": 100})
    );
    let unknown = Tool {
        name: "t".to_owned(),
        input_schema: Value::Null,
    };
    assert_eq!(
        unknown.args(&[(&["id", "issueId"], json!("ENG-1"))]),
        json!({"id": "ENG-1"})
    );
}

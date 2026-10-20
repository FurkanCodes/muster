use serde_json::json;

use super::*;

fn project() -> Project {
    Project::new(
        Path::new("/work/app"),
        Some("github.com/o/app".to_owned()),
        BTreeMap::from([("site".to_owned(), "acme".to_owned())]),
    )
    .unwrap()
}

fn key(text: &str) -> TicketKey {
    TicketKey::parse(text).unwrap()
}

#[test]
fn requests_are_one_json_rpc_line_each() {
    let hello = request_line(
        1,
        &Call::Hello {
            muster: "0.4.0".to_owned(),
            protocol: PROTOCOL,
        },
    );
    assert_eq!(
        hello,
        "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"hello\",\"params\":{\"muster\":\"0.4.0\",\"protocol\":1}}\n"
    );
    let list = request_line(
        2,
        &Call::List {
            project: project(),
            mine: true,
        },
    );
    assert_eq!(list.matches('\n').count(), 1);
    let value: Value = serde_json::from_str(&list).unwrap();
    assert_eq!(
        value,
        json!({"jsonrpc": "2.0", "id": 2, "method": "list", "params": {
            "project": {"root": "/work/app", "remote": "github.com/o/app", "settings": {"site": "acme"}},
            "mine": true}})
    );
}

#[test]
fn a_plugin_reads_each_request_back() {
    for call in [
        Call::Hello {
            muster: "0.4.0".to_owned(),
            protocol: 1,
        },
        Call::List {
            project: project(),
            mine: false,
        },
        Call::View {
            project: project(),
            key: key("APP-7"),
        },
        Call::Close {
            project: project(),
            key: key("42"),
        },
    ] {
        let line = request_line(9, &call);
        assert_eq!(
            read_request(line.trim_end().as_bytes()),
            Ok(Request { id: 9, call })
        );
    }
    // What it cannot read it answers by id where it can.
    assert_eq!(read_request(b"nonsense").unwrap_err().0, None);
    let unknown = read_request(br#"{"jsonrpc":"2.0","id":4,"method":"delete","params":{}}"#);
    assert_eq!(unknown.unwrap_err().0, Some(4));
    let bad_key = read_request(
        br#"{"id":5,"method":"view","params":{"project":{"root":"/a"},"key":"$(rm -rf ~)"}}"#,
    );
    assert_eq!(bad_key.unwrap_err().0, Some(5));
    assert!(read_request(br#"{"method":"hello","params":{"muster":"0","protocol":1}}"#).is_err());
}

#[test]
fn replies_carry_a_result_or_an_error() {
    let ok = result_line(3, &HelloResult { protocol: 1 });
    assert_eq!(
        ok,
        "{\"jsonrpc\":\"2.0\",\"id\":3,\"result\":{\"protocol\":1}}\n"
    );
    assert_eq!(
        read_reply(ok.as_bytes()),
        Ok(Reply {
            id: Some(3),
            outcome: Ok(json!({"protocol": 1}))
        })
    );
    let refusal = PluginError {
        code: PluginErrorCode::SignedOut,
        message: "run gh auth login".to_owned(),
    };
    let line = error_line(Some(4), &refusal);
    assert!(line.contains(r#""code":"signed_out""#), "{line}");
    assert_eq!(read_reply(line.as_bytes()).unwrap().outcome, Err(refusal));
    // Every code there is, and plain JSON-RPC's numbers, which say less.
    for (code, read) in [
        (json!("not_found"), PluginErrorCode::NotFound),
        (json!("refused"), PluginErrorCode::Refused),
        (json!("signed_out"), PluginErrorCode::SignedOut),
        (json!("not_configured"), PluginErrorCode::NotConfigured),
        (json!("failed"), PluginErrorCode::Failed),
        (json!("on_fire"), PluginErrorCode::Failed),
        (json!(-32601), PluginErrorCode::Failed),
    ] {
        let line = json!({"id": 1, "error": {"code": code, "message": "m"}}).to_string();
        let Err(error) = read_reply(line.as_bytes()).unwrap().outcome else {
            panic!("expected an error");
        };
        assert_eq!(error.code, read, "{code}");
    }
    // Its reason loses control characters and is cut.
    let loud = json!({"id": 1, "error": {"code": "failed", "message": format!("a\u{1b}[2Jb\n{}", "x".repeat(10_000))}});
    let Err(error) = read_reply(loud.to_string().as_bytes()).unwrap().outcome else {
        panic!("expected an error");
    };
    assert!(error.message.starts_with("a[2Jb\nxx"));
    assert_eq!(error.message.len(), MOST_MESSAGE);
    // A request it could not read is answered with no id.
    assert_eq!(
        read_reply(br#"{"id":null,"error":{"code":"failed","message":"?"}}"#)
            .unwrap()
            .id,
        None
    );
}

#[test]
fn anything_else_on_a_plugin_s_stdout_is_not_a_reply() {
    for garbage in [
        &b"hello there"[..],
        b"[1,2]",
        br#"{"id":1}"#,
        br#"{"id":"one","result":{}}"#,
        br#"{"id":-1,"result":{}}"#,
    ] {
        assert!(
            read_reply(garbage).is_err(),
            "{}",
            String::from_utf8_lossy(garbage)
        );
    }
}

#[test]
fn hello_says_the_plugin_s_protocol() {
    assert_eq!(hello(json!({"protocol": 1, "name": "YouTrack"})), Ok(1));
    assert_eq!(hello(json!({"protocol": 2})), Ok(2));
    assert!(hello(json!({})).is_err());
}

#[test]
fn a_listing_keeps_the_tickets_it_can_take_and_counts_the_rest() {
    let listing = json!({
        "where": "YouTrack (acme.myjetbrains.com)",
        "source": "youtrack acme APP",
        "tickets": [
            {"key": "APP-7", "title": "Fix it", "url": "https://y/APP-7", "labels": ["bug"], "author": "someone"},
            {"key": 42, "title": "A number for its key"},
            {"key": "$(rm -rf ~)", "title": "Not a key"},
            {"key": "x-y-1", "title": "Not a key either"},
            {"key": "APP-8"},
            {"key": "APP-9", "title": 9},
            "not a ticket",
        ]
    });
    let listed = listed(listing).unwrap();
    assert_eq!(
        listed.tracker.as_deref(),
        Some("YouTrack (acme.myjetbrains.com)")
    );
    assert_eq!(listed.source.as_deref(), Some("youtrack acme APP"));
    assert_eq!(listed.dropped, 5);
    assert_eq!(
        listed.tickets,
        [
            Issue {
                key: key("APP-7"),
                title: "Fix it".to_owned(),
                url: "https://y/APP-7".to_owned(),
                labels: vec!["bug".to_owned()],
                author: Some("someone".to_owned()),
            },
            Issue {
                key: key("42"),
                title: "A number for its key".to_owned(),
                url: String::new(),
                labels: Vec::new(),
                author: None,
            },
        ]
    );
    // Where it said nothing of where they are, the plugin's name says it.
    let quiet = listed_of(json!({"tickets": []}));
    assert_eq!((quiet.tracker, quiet.source), (None, None));
    assert_eq!(
        listed_of(json!({"where": " \u{1b}\n", "tickets": []})).tracker,
        None
    );
    assert!(super::listed(json!({"where": "x"})).is_err());
    assert!(super::listed(json!([])).is_err());
}

fn listed_of(value: Value) -> Listed {
    listed(value).unwrap()
}

#[test]
fn a_listing_keeps_so_many_tickets() {
    let tickets: Vec<_> = (1..=MOST_TICKETS + 5)
        .map(|n| json!({"key": n.to_string(), "title": "t"}))
        .collect();
    let listed = listed_of(json!({"tickets": tickets}));
    assert_eq!(listed.tickets.len(), MOST_TICKETS);
    assert_eq!(listed.dropped, 5);
}

#[test]
fn ticket_text_loses_control_characters_and_is_cut() {
    let listed = listed_of(json!({"tickets": [{
        "key": "7",
        "title": format!("Evil\u{1b}]0;owned\u{7} title\nhere{}", "é".repeat(MOST_TITLE)),
        "url": "https://x/7\u{1b}[2J",
        "labels": (0..80).map(|n| format!("l{n}\u{9b}")).collect::<Vec<_>>(),
        "author": "m\u{1b}[31mal",
    }]}));
    let issue = &listed.tickets[0];
    assert!(issue.title.starts_with("Evil]0;owned title here"));
    assert!(issue.title.len() <= MOST_TITLE);
    assert_eq!(issue.url, "https://x/7[2J");
    assert_eq!(issue.labels.len(), MOST_LABELS);
    assert_eq!(issue.labels[0], "l0");
    assert_eq!(issue.author.as_deref(), Some("m[31mal"));
}

#[test]
fn a_viewed_ticket_is_the_one_asked_for_with_its_text_cleaned() {
    let viewing = json!({
        "ticket": {"key": "app-7", "title": "Fix it", "url": "https://y/APP-7"},
        "body": format!("line\r\nnext\u{1b}[31m{}", "b".repeat(MOST_BODY)),
    });
    let viewed = viewed(viewing.clone(), &key("APP-7")).unwrap();
    assert_eq!(viewed.issue.key, key("APP-7"));
    assert!(viewed.body.starts_with("line\nnext[31mbbb"));
    assert_eq!(viewed.body.len(), MOST_BODY);
    // Another ticket than the one asked for is not taken for it.
    let other = super::viewed(viewing, &key("APP-8")).unwrap_err();
    assert!(other.contains("asked for APP-8"), "{other}");
    assert!(super::viewed(json!({"ticket": {"key": "?", "title": "t"}}), &key("1")).is_err());
    // No text is none.
    let bare = super::viewed(json!({"ticket": {"key": "1", "title": "t"}}), &key("1")).unwrap();
    assert_eq!(bare.body, "");
}

#[test]
fn a_plugin_writes_what_muster_reads() {
    let issue = Issue {
        key: key("42"),
        title: "Fix it".to_owned(),
        url: "https://github.com/o/app/issues/42".to_owned(),
        labels: vec!["bug".to_owned()],
        author: Some("furkan".to_owned()),
    };
    let listing = Listing {
        tracker: "GitHub issues".to_owned(),
        source: None,
        tickets: vec![Ticket::from(&issue)],
    };
    let line = result_line(2, &listing);
    let Reply {
        outcome: Ok(result),
        ..
    } = read_reply(line.as_bytes()).unwrap()
    else {
        panic!("expected a result");
    };
    let listed = listed(result).unwrap();
    assert_eq!(listed.tracker.as_deref(), Some("GitHub issues"));
    assert_eq!(listed.tickets, std::slice::from_ref(&issue));
    let viewing = Viewing {
        ticket: Ticket::from(&issue),
        body: "text".to_owned(),
    };
    let value = serde_json::to_value(&viewing).unwrap();
    assert_eq!(
        viewed(value, &key("42")).unwrap(),
        Viewed {
            issue,
            body: "text".to_owned()
        }
    );
}

#[test]
fn a_folder_json_cannot_carry_is_no_project() {
    use std::os::unix::ffi::OsStrExt;
    let odd = Path::new(std::ffi::OsStr::from_bytes(b"/work/\xff"));
    assert_eq!(Project::new(odd, None, BTreeMap::new()), None);
}

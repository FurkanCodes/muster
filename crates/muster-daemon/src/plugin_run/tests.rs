use std::path::Path;
use std::sync::Mutex;

use muster_core::ticket_protocol::{Project, Reply, read_reply, request_line};
use muster_core::{Issue, TicketKey};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use super::*;
use crate::service::BoxFuture;
use crate::trackers::IssueDetail;

/// A repository's tickets as a test keeps them: issue 42, and what was
/// asked of them.
#[derive(Debug, Default)]
struct Kept {
    asked: Mutex<Vec<String>>,
    /// Holds a view of issue 1 until told.
    release: tokio::sync::Notify,
}

#[derive(Debug)]
struct Repository {
    kept: Arc<Kept>,
    root: PathBuf,
    mine: bool,
}

fn issue(number: u64) -> Issue {
    Issue {
        key: TicketKey::number(number),
        title: "Fix the login redirect".to_owned(),
        url: format!("https://github.com/o/app/issues/{number}"),
        labels: vec!["bug".to_owned()],
        author: Some("furkan".to_owned()),
    }
}

impl TicketSource for Repository {
    fn describe(&self) -> String {
        "GitHub issues".to_owned()
    }

    fn list(&self) -> BoxFuture<'_, Result<Vec<Issue>, ServiceError>> {
        self.kept.asked.lock().unwrap().push(format!(
            "list {} mine={}",
            self.root.display(),
            self.mine
        ));
        Box::pin(async { Ok(vec![issue(42)]) })
    }

    fn view<'a>(&'a self, key: &'a TicketKey) -> BoxFuture<'a, Result<IssueDetail, ServiceError>> {
        Box::pin(async move {
            match key.as_number() {
                Some(1) => self.kept.release.notified().await,
                Some(42) => {}
                _ => {
                    return Err(ServiceError::NotFound(format!(
                        "GraphQL: Could not resolve to an Issue with the number of {key}."
                    )));
                }
            }
            Ok(IssueDetail {
                issue: issue(key.as_number().unwrap_or_default()),
                body: "After signing in, users land on /404.".to_owned(),
                noun: crate::trackers::Noun::own("github"),
            })
        })
    }

    fn close<'a>(&'a self, key: &'a TicketKey) -> BoxFuture<'a, Result<(), ServiceError>> {
        self.kept.asked.lock().unwrap().push(format!("close {key}"));
        Box::pin(async {
            Err(ServiceError::NotSignedIn(
                "gh: To get started with GitHub CLI, please run:  gh auth login".to_owned(),
            ))
        })
    }
}

fn project() -> Project {
    Project::new(
        Path::new("/work/app"),
        None,
        std::collections::BTreeMap::new(),
    )
    .unwrap()
}

/// A plugin served over pipes, its tickets kept by the test.
struct Served {
    kept: Arc<Kept>,
    to: tokio::io::DuplexStream,
    from: tokio::io::Lines<BufReader<tokio::io::DuplexStream>>,
    serving: tokio::task::JoinHandle<std::io::Result<()>>,
}

fn served() -> Served {
    let kept = Arc::new(Kept::default());
    let sources: Arc<Sources> = {
        let kept = kept.clone();
        Arc::new(move |root, mine| -> Arc<dyn TicketSource> {
            Arc::new(Repository {
                kept: kept.clone(),
                root,
                mine,
            })
        })
    };
    let (to, input) = tokio::io::duplex(64 * 1024);
    let (output, from) = tokio::io::duplex(64 * 1024);
    #[expect(clippy::disallowed_methods, reason = "ARC-13: the test awaits it")]
    let serving = tokio::spawn(serve(sources, input, output));
    Served {
        kept,
        to,
        from: BufReader::new(from).lines(),
        serving,
    }
}

impl Served {
    async fn send(&mut self, line: &str) {
        self.to.write_all(line.as_bytes()).await.unwrap();
    }

    async fn ask(&mut self, id: u64, call: Call) {
        self.send(&request_line(id, &call)).await;
    }

    async fn reply(&mut self) -> Reply {
        let line = self.from.next_line().await.unwrap().unwrap();
        read_reply(line.as_bytes()).unwrap()
    }
}

fn result(reply: Reply) -> Value {
    reply.outcome.unwrap()
}

#[tokio::test]
async fn each_method_is_answered() {
    let mut plugin = served();
    plugin
        .ask(
            1,
            Call::Hello {
                muster: "0.4.0".to_owned(),
                protocol: PROTOCOL,
            },
        )
        .await;
    let hello = plugin.reply().await;
    assert_eq!(hello.id, Some(1));
    assert_eq!(result(hello), json!({"protocol": 1}));

    plugin
        .ask(
            2,
            Call::List {
                project: project(),
                mine: true,
            },
        )
        .await;
    let listed = plugin.reply().await;
    assert_eq!(listed.id, Some(2));
    assert_eq!(
        result(listed),
        json!({"where": "GitHub issues", "source": null, "tickets": [{
            "key": "42", "title": "Fix the login redirect",
            "url": "https://github.com/o/app/issues/42", "labels": ["bug"], "author": "furkan"}]})
    );

    plugin
        .ask(
            3,
            Call::View {
                project: project(),
                key: TicketKey::number(42),
            },
        )
        .await;
    let viewed = result(plugin.reply().await);
    assert_eq!(viewed["ticket"]["key"], "42");
    assert_eq!(viewed["body"], "After signing in, users land on /404.");

    // Muster's errors go as the protocol's, their messages as they are.
    plugin
        .ask(
            4,
            Call::View {
                project: project(),
                key: TicketKey::number(999),
            },
        )
        .await;
    let missing = plugin.reply().await.outcome.unwrap_err();
    assert_eq!(missing.code, PluginErrorCode::NotFound);
    assert!(missing.message.contains("Could not resolve"), "{missing:?}");
    plugin
        .ask(
            5,
            Call::Close {
                project: project(),
                key: TicketKey::number(42),
            },
        )
        .await;
    let signed_out = plugin.reply().await.outcome.unwrap_err();
    assert_eq!(signed_out.code, PluginErrorCode::SignedOut);
    assert!(signed_out.message.contains("gh auth login"));
    assert_eq!(
        *plugin.kept.asked.lock().unwrap(),
        ["list /work/app mine=true", "close 42"]
    );

    // What it cannot read it says so of, by id where it has one; blank
    // lines are passed over.
    plugin.send("\n").await;
    plugin.send("garbage\n").await;
    let garbage = plugin.reply().await;
    assert_eq!(garbage.id, None);
    assert_eq!(garbage.outcome.unwrap_err().code, PluginErrorCode::Failed);
    plugin
        .send("{\"jsonrpc\":\"2.0\",\"id\":6,\"method\":\"delete\",\"params\":{}}\n")
        .await;
    let unknown = plugin.reply().await;
    assert_eq!(unknown.id, Some(6));
    assert!(unknown.outcome.is_err());

    // It ends when its stdin does.
    drop(plugin.to);
    plugin.serving.await.unwrap().unwrap();
}

#[tokio::test]
async fn a_slow_request_holds_up_no_other() {
    let mut plugin = served();
    plugin
        .ask(
            1,
            Call::View {
                project: project(),
                key: TicketKey::number(1),
            },
        )
        .await;
    plugin
        .ask(
            2,
            Call::Hello {
                muster: "0.4.0".to_owned(),
                protocol: PROTOCOL,
            },
        )
        .await;
    assert_eq!(plugin.reply().await.id, Some(2));
    plugin.kept.release.notify_one();
    let viewed = plugin.reply().await;
    assert_eq!(viewed.id, Some(1));
    assert_eq!(result(viewed)["ticket"]["key"], "1");
}

#[tokio::test]
async fn only_its_own_plugins_are_served() {
    let error = plugin_run("youtrack").await.unwrap_err();
    assert!(matches!(error, PluginRunError::Unknown(_)));
    assert!(error.to_string().contains("github and gitlab"));
}

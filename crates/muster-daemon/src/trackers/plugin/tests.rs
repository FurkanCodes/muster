//! `PluginTickets` against small `sh` stand-ins for a ticket plugin, each
//! answering by the method a request names.

use std::collections::BTreeMap;
use std::time::Duration;

use super::process::{Limits, MOST_IN_FLIGHT};
use super::*;
use crate::trackers::Hidden;

/// The loop every stand-in runs: it counts its starts and ends, records
/// each request, and answers with the functions before it, `hello`, `list`
/// and `view`, which see the request's `$id` and `$line`.
const LOOP: &str = r#"
dir="$1"
echo start >> "$dir/starts"
starts=$(wc -l < "$dir/starts" | tr -d ' ')
env > "$dir/env"
while IFS= read -r line; do
  printf '%s\n' "$line" >> "$dir/requests"
  id=$(printf '%s' "$line" | sed -n 's/^{"jsonrpc":"2.0","id":\([0-9]*\),.*/\1/p')
  case "$line" in
    *'"method":"hello"'*) hello ;;
    *'"method":"list"'*) list ;;
    *'"method":"view"'*) view ;;
    *'"method":"close"'*) printf '{"jsonrpc":"2.0","id":%s,"result":{}}\n' "$id" ;;
  esac
done
echo end >> "$dir/ends"
"#;

const HELLO: &str =
    r#"hello() { printf '{"jsonrpc":"2.0","id":%s,"result":{"protocol":1}}\n' "$id"; }"#;

const LIST: &str = r#"list() { printf '{"jsonrpc":"2.0","id":%s,"result":{"where":"Fake (acme)","source":"fake acme APP","tickets":[{"key":"APP-7","title":"Fix it","url":"https://f/APP-7","labels":["bug"],"author":"someone"}]}}\n' "$id"; }"#;

const VIEW: &str = r#"view() { printf '{"jsonrpc":"2.0","id":%s,"result":{"ticket":{"key":"APP-7","title":"Fix it","url":"https://f/APP-7"},"body":"The text."}}\n' "$id"; }"#;

/// A stand-in plugin in a folder of its own.
struct Fake {
    dir: tempfile::TempDir,
    plugins: Plugins,
}

impl Fake {
    /// A plugin whose `hello`, `list` and `view` are these, or the plain
    /// ones where empty.
    fn new(hello: &str, list: &str, view: &str) -> Self {
        Self::with_limits(
            hello,
            list,
            view,
            Limits {
                hello: Duration::from_secs(5),
                request: Duration::from_secs(5),
                idle: Duration::from_secs(60),
            },
        )
    }

    fn with_limits(hello: &str, list: &str, view: &str, limits: Limits) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let pick = |given: &str, plain: &str| {
            if given.is_empty() {
                plain.to_owned()
            } else {
                given.to_owned()
            }
        };
        let script = format!(
            "{}\n{}\n{}\n{LOOP}",
            pick(hello, HELLO),
            pick(list, LIST),
            pick(view, VIEW)
        );
        std::fs::write(dir.path().join("plugin.sh"), script).unwrap();
        Self {
            dir,
            plugins: Plugins::new("muster".into()).with_limits(limits),
        }
    }

    fn spec(&self) -> PluginSpec {
        PluginSpec {
            id: "fake".to_owned(),
            name: "Fake".to_owned(),
            program: "/bin/sh".into(),
            args: vec![
                self.dir.path().join("plugin.sh").display().to_string(),
                self.dir.path().display().to_string(),
            ],
            tracker: TrackerKind::Jira,
        }
    }

    fn tickets(&self, mine: bool) -> PluginTickets {
        let project = Project::new(
            Path::new("/work/app"),
            Some("github.com/o/app".to_owned()),
            BTreeMap::new(),
        )
        .unwrap();
        PluginTickets::new(self.plugins.process(self.spec()), project, mine)
    }

    fn read(&self, file: &str) -> String {
        std::fs::read_to_string(self.dir.path().join(file)).unwrap_or_default()
    }

    fn count(&self, file: &str) -> usize {
        self.read(file).lines().count()
    }

    /// Waits, a few seconds at most, until `done`.
    async fn until(&self, done: impl Fn(&Self) -> bool) {
        for _ in 0..300 {
            if done(self) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("waited in vain; requests:\n{}", self.read("requests"));
    }
}

fn key(text: &str) -> TicketKey {
    TicketKey::parse(text).unwrap()
}

/// The message of an error the plugin's process gave.
fn unavailable(error: ServiceError) -> String {
    match error {
        ServiceError::Unavailable(message) => message,
        other => panic!("expected unavailable, got {other:?}"),
    }
}

#[tokio::test]
async fn tickets_are_listed_viewed_and_closed_through_a_plugin() {
    let fake = Fake::new("", "", "");
    let tickets = fake.tickets(true);
    assert_eq!(tickets.describe(), "Fake", "before a listing, its name");
    let listed = tickets.list().await.unwrap();
    assert_eq!(
        listed,
        [Issue {
            key: key("APP-7"),
            title: "Fix it".to_owned(),
            url: "https://f/APP-7".to_owned(),
            labels: vec!["bug".to_owned()],
            author: Some("someone".to_owned()),
        }]
    );
    assert_eq!(tickets.describe(), "Fake (acme)");
    assert_eq!(tickets.source().as_deref(), Some("fake acme APP"));
    assert!(tickets.only_mine());
    let detail = tickets.view(&key("APP-7")).await.unwrap();
    assert_eq!(detail.body, "The text.");
    assert_eq!(detail.tracker, TrackerKind::Jira);
    tickets.close(&key("APP-7")).await.unwrap();

    // One process, said hello to once, each request with an id of its own.
    assert_eq!(fake.count("starts"), 1);
    let requests = fake.read("requests");
    let methods: Vec<_> = requests
        .lines()
        .map(|line| {
            let value: serde_json::Value = serde_json::from_str(line).unwrap();
            (
                value["id"].as_u64().unwrap(),
                value["method"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    assert_eq!(
        methods,
        [
            (1, "hello".to_owned()),
            (2, "list".to_owned()),
            (3, "view".to_owned()),
            (4, "close".to_owned())
        ]
    );
    assert!(requests.contains(r#""mine":true"#), "{requests}");
    assert!(
        requests.contains(
            r#""project":{"root":"/work/app","remote":"github.com/o/app","settings":{}}"#
        ),
        "{requests}"
    );
    // Another project's tickets from the same plugin use its process.
    fake.tickets(false).list().await.unwrap();
    assert_eq!(fake.count("starts"), 1);
}

#[tokio::test]
async fn a_hang_hits_its_deadline_and_the_next_request_starts_it_again() {
    let hangs_first = r#"list() { if [ "$starts" -eq 1 ]; then sleep 5; else printf '{"jsonrpc":"2.0","id":%s,"result":{"tickets":[]}}\n' "$id"; fi; }"#;
    let fake = Fake::with_limits(
        "",
        hangs_first,
        "",
        Limits {
            hello: Duration::from_secs(5),
            request: Duration::from_secs(1),
            idle: Duration::from_secs(60),
        },
    );
    let tickets = fake.tickets(false);
    let hung = unavailable(tickets.list().await.unwrap_err());
    assert_eq!(hung, "the Fake plugin did not answer within 1 s");
    assert_eq!(tickets.list().await.unwrap(), []);
    assert_eq!(fake.count("starts"), 2);
}

#[tokio::test]
async fn a_crash_answers_with_why_and_the_next_request_starts_it_again() {
    let crashes_first = r#"list() { if [ "$starts" -eq 1 ]; then echo "panic: the tracker went away" >&2; exit 3; else printf '{"jsonrpc":"2.0","id":%s,"result":{"tickets":[]}}\n' "$id"; fi; }"#;
    let fake = Fake::new("", crashes_first, "");
    let tickets = fake.tickets(false);
    let crashed = unavailable(tickets.list().await.unwrap_err());
    assert_eq!(
        crashed,
        "the Fake plugin ended (exit status: 3); it said: panic: the tracker went away"
    );
    assert_eq!(tickets.list().await.unwrap(), []);
    assert_eq!(fake.count("starts"), 2);
    let process = fake.plugins.process(fake.spec());
    assert!(process.log().contains("panic: the tracker went away"));
}

#[tokio::test]
async fn a_line_that_is_no_reply_stops_it() {
    let chatty_first = r#"list() { if [ "$starts" -eq 1 ]; then echo "Listing tickets..."; else printf '{"jsonrpc":"2.0","id":%s,"result":{"tickets":[]}}\n' "$id"; fi; }"#;
    let fake = Fake::new("", chatty_first, "");
    let tickets = fake.tickets(false);
    let garbage = unavailable(tickets.list().await.unwrap_err());
    assert!(
        garbage.starts_with("the Fake plugin answered something that is not the ticket protocol"),
        "{garbage}"
    );
    assert_eq!(tickets.list().await.unwrap(), []);
    assert_eq!(fake.count("starts"), 2);
}

#[tokio::test]
async fn a_reply_too_long_stops_it() {
    let long_first = r#"list() { if [ "$starts" -eq 1 ]; then head -c 4200000 /dev/zero | tr '\0' a; echo; else printf '{"jsonrpc":"2.0","id":%s,"result":{"tickets":[]}}\n' "$id"; fi; }"#;
    let fake = Fake::new("", long_first, "");
    let tickets = fake.tickets(false);
    let long = unavailable(tickets.list().await.unwrap_err());
    assert_eq!(long, "the Fake plugin answered more than 4 MiB at once");
    assert_eq!(tickets.list().await.unwrap(), []);
}

#[tokio::test]
async fn tickets_it_cannot_take_are_dropped_and_their_text_cleaned() {
    let odd = r#"list() { printf '{"jsonrpc":"2.0","id":%s,"result":{"tickets":[{"key":"$(rm -rf ~)","title":"Not a key"},{"key":"APP-7","title":"Evil\\u001b]0;owned\\u0007 title\\nhere","labels":["b\\u001b[2Jug"],"author":"m\\u009bal"},{"title":"No key"}]}}\n' "$id"; }"#;
    let view = r#"view() { printf '{"jsonrpc":"2.0","id":%s,"result":{"ticket":{"key":"APP-7","title":"t"},"body":"line\\r\\nnext\\u001b[31m"}}\n' "$id"; }"#;
    let fake = Fake::new("", odd, view);
    let tickets = fake.tickets(false);
    let listed = tickets.list().await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].key, key("APP-7"));
    assert_eq!(listed[0].title, "Evil]0;owned title here");
    assert_eq!(listed[0].labels, ["b[2Jug"]);
    assert_eq!(listed[0].author.as_deref(), Some("mal"));
    // Where it said nothing of where they are, its name says it.
    assert_eq!(tickets.describe(), "Fake");
    assert_eq!(tickets.source(), None);
    assert_eq!(
        tickets.view(&key("APP-7")).await.unwrap().body,
        "line\nnext[31m"
    );
}

#[tokio::test]
async fn a_ticket_s_credentials_are_hidden_as_before() {
    let pasted = r#"list() { printf '{"jsonrpc":"2.0","id":%s,"result":{"tickets":[{"key":"APP-7","title":"Rotate ghp_abcdefghijklmnopqrstuvwxyz0123"}]}}\n' "$id"; }"#;
    let view = r#"view() { printf '{"jsonrpc":"2.0","id":%s,"result":{"ticket":{"key":"APP-7","title":"t"},"body":"curl -H %s https://api.example.com"}}\n' "$id" "'x-api-key: 00000000-0000-4000-8000-000000000000'"; }"#;
    let fake = Fake::new("", pasted, view);
    let hidden = Hidden(Arc::new(fake.tickets(false)));
    assert_eq!(
        hidden.list().await.unwrap()[0].title,
        "Rotate [hidden by Muster]"
    );
    assert_eq!(
        hidden.view(&key("APP-7")).await.unwrap().body,
        "curl -H 'x-api-key: [hidden by Muster]' https://api.example.com"
    );
}

#[tokio::test]
async fn its_refusals_say_what_kind_they_are() {
    let refuses = r#"list() { printf '{"jsonrpc":"2.0","id":%s,"error":{"code":"signed_out","message":"run fake login"}}\n' "$id"; }"#;
    let view = r#"view() { case "$line" in *APP-9*) printf '{"jsonrpc":"2.0","id":%s,"error":{"code":"failed","message":""}}\n' "$id" ;; *APP-8*) printf '{"jsonrpc":"2.0","id":%s,"result":{"ticket":{"key":"APP-1","title":"t"}}}\n' "$id" ;; *) printf '{"jsonrpc":"2.0","id":%s,"error":{"code":"not_found","message":"no APP-7 here"}}\n' "$id" ;; esac; }"#;
    let fake = Fake::new("", refuses, view);
    let tickets = fake.tickets(false);
    let signed_out = tickets.list().await.unwrap_err();
    assert!(
        matches!(&signed_out, ServiceError::NotSignedIn(message) if message == "run fake login"),
        "{signed_out:?}"
    );
    assert!(matches!(
        tickets.view(&key("APP-7")).await.unwrap_err(),
        ServiceError::NotFound(message) if message == "no APP-7 here"
    ));
    assert_eq!(
        unavailable(tickets.view(&key("APP-9")).await.unwrap_err()),
        "the Fake plugin failed and said nothing of why"
    );
    // Another ticket than the one asked for is not taken for it.
    let other = tickets.view(&key("APP-8")).await.unwrap_err();
    assert!(
        matches!(&other, ServiceError::Unreadable(message) if message.contains("asked for APP-8")),
        "{other:?}"
    );
    // None of it is a crash.
    assert_eq!(fake.count("starts"), 1);
}

#[tokio::test]
async fn a_plugin_that_speaks_another_protocol_is_refused() {
    let newer =
        r#"hello() { printf '{"jsonrpc":"2.0","id":%s,"result":{"protocol":2}}\n' "$id"; }"#;
    let fake = Fake::new(newer, "", "");
    let refused = unavailable(fake.tickets(false).list().await.unwrap_err());
    assert_eq!(
        refused,
        "the Fake plugin speaks version 2 of the ticket protocol; this Muster speaks 1"
    );
}

#[tokio::test]
async fn a_program_that_is_not_there_says_so() {
    let fake = Fake::new("", "", "");
    let mut spec = fake.spec();
    spec.program = "/nowhere/plugin".into();
    let project = Project::new(Path::new("/work/app"), None, BTreeMap::new()).unwrap();
    let tickets = PluginTickets::new(fake.plugins.process(spec), project, false);
    let missing = unavailable(tickets.list().await.unwrap_err());
    assert!(
        missing.starts_with("could not start the Fake plugin (/nowhere/plugin)"),
        "{missing}"
    );
}

#[tokio::test]
async fn at_most_eight_requests_wait_on_it() {
    // Its first process reads every request and answers none.
    let silent_first = r#"list() { if [ "$starts" -gt 1 ]; then printf '{"jsonrpc":"2.0","id":%s,"result":{"tickets":[]}}\n' "$id"; fi; }"#;
    let fake = Fake::with_limits(
        "",
        silent_first,
        "",
        Limits {
            hello: Duration::from_secs(5),
            request: Duration::from_secs(3),
            idle: Duration::from_secs(60),
        },
    );
    let tickets = Arc::new(fake.tickets(false));
    let mut asking = tokio::task::JoinSet::new();
    for _ in 0..MOST_IN_FLIGHT + 4 {
        let tickets = tickets.clone();
        asking.spawn(async move { tickets.list().await.is_ok() });
    }
    let lists = |fake: &Fake| fake.read("requests").matches(r#""method":"list""#).count();
    fake.until(|fake| lists(fake) >= MOST_IN_FLIGHT).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(lists(&fake), MOST_IN_FLIGHT, "the rest wait their turn");
    // The eight let their deadline pass, the plugin is taken as hung, and
    // the four waiting go to its next process.
    let answered: Vec<bool> = asking.join_all().await;
    assert_eq!(answered.iter().filter(|ok| !**ok).count(), MOST_IN_FLIGHT);
    assert_eq!(answered.iter().filter(|ok| **ok).count(), 4);
    assert_eq!(fake.count("starts"), 2);
}

#[tokio::test]
async fn it_stops_when_unused_and_starts_again_when_asked() {
    let fake = Fake::with_limits(
        "",
        "",
        "",
        Limits {
            hello: Duration::from_secs(5),
            request: Duration::from_secs(5),
            idle: Duration::from_millis(300),
        },
    );
    let tickets = fake.tickets(false);
    tickets.list().await.unwrap();
    fake.until(|fake| fake.count("ends") == 1).await;
    tickets.list().await.unwrap();
    assert_eq!(fake.count("starts"), 2);
}

#[tokio::test]
async fn it_stops_with_the_daemon() {
    let fake = Fake::new("", "", "");
    fake.tickets(false).list().await.unwrap();
    assert_eq!(fake.count("ends"), 0);
    fake.plugins.stop().await;
    assert_eq!(fake.count("ends"), 1, "it was let finish, not killed");
}

#[tokio::test]
async fn its_log_keeps_the_end_of_its_stderr_and_its_environment_is_a_session_s() {
    let loud = r#"hello() { head -c 70000 /dev/zero | tr '\0' x >&2; echo " last words" >&2; printf '{"jsonrpc":"2.0","id":%s,"result":{"protocol":1}}\n' "$id"; }"#;
    let fake = Fake::new(loud, "", "");
    fake.tickets(false).list().await.unwrap();
    let process = fake.plugins.process(fake.spec());
    for _ in 0..300 {
        if process.log().ends_with("last words\n") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let log = process.log();
    assert!(log.ends_with("x last words\n"));
    assert_eq!(log.len(), super::streams::MOST_LOG);

    // The person's PATH, and none of what a session never inherits.
    let env = fake.read("env");
    let path = std::env::var("PATH").unwrap();
    assert!(env.lines().any(|line| line == format!("PATH={path}")));
    let ours = std::env::vars_os().filter_map(|(key, _)| key.into_string().ok());
    for key in crate::harness::scrub::scrubbed(ours) {
        assert!(
            !env.lines().any(|line| line.starts_with(&format!("{key}="))),
            "{key} reached the plugin"
        );
    }
}

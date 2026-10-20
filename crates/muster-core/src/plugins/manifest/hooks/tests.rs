use super::super::{Manifest, ManifestError, read};
use crate::WaitReason;
use crate::plugins::events::EventKind;
use crate::plugins::hooks::{TIMEOUT, When};

/// A plugin of hooks alone, as the task's example writes one.
const HOOKS: &str = r#"
id = "notify"
name = "Notify"
version = "0.1.0"
muster = "0.4.0"
secrets = ["WEBHOOK_URL"]

[[hooks]]
id = "ping"
on = ["session.needs_you", "session.finished"]
command = ["./notify", "{project}", "{branch}"]
timeout = 20
[hooks.when]
project = ["muster"]
harness = ["claude", "codex"]
reason = ["question", "permission"]
away = true

[[hooks]]
id = "log"
on = ["worktree.created", "worktree.removed", "pull_request.changed", "clash.found"]
command = ["./log", "{workspace}"]
"#;

fn manifest(text: &str) -> Result<Manifest, ManifestError> {
    read(text, "0.4.0", false)
}

fn error(text: &str) -> String {
    manifest(text).unwrap_err().to_string()
}

/// `HOOKS` with `old` replaced by `new`.
fn changed(old: &str, new: &str) -> String {
    assert!(HOOKS.contains(old), "{old}");
    HOOKS.replacen(old, new, 1)
}

#[test]
fn a_plugin_of_hooks_alone_reads_with_every_key() {
    let read = manifest(HOOKS).unwrap();
    assert!(read.panels.is_empty() && read.tickets.is_none());
    let [ping, log] = read.hooks.as_slice() else {
        panic!("{:?}", read.hooks);
    };
    assert_eq!(ping.id, "ping");
    assert_eq!(
        ping.on,
        [EventKind::SessionNeedsYou, EventKind::SessionFinished]
    );
    assert_eq!(ping.command, ["./notify", "{project}", "{branch}"]);
    assert_eq!(ping.timeout, 20);
    assert_eq!(
        ping.when,
        When {
            project: vec!["muster".to_owned()],
            harness: vec!["claude".to_owned(), "codex".to_owned()],
            reason: vec![WaitReason::Question, WaitReason::Permission],
            away: Some(true),
        }
    );
    assert_eq!(log.timeout, TIMEOUT);
    assert!(log.when.is_empty());
    let commands: Vec<String> = read.commands().into_iter().map(|(at, _)| at).collect();
    assert_eq!(commands, ["hooks[0].command", "hooks[1].command"]);
}

#[test]
fn an_unknown_key_or_event_is_refused_naming_where_it_is() {
    assert!(
        error(&changed("timeout = 20", "timeout = 20\nretries = 3"))
            .starts_with("hooks[0].retries is not a key"),
    );
    assert!(
        error(&changed("away = true", "away = true\nquiet = 1")).starts_with(
            "hooks[0].when.quiet is not a key of a plugin's manifest here: they are project, \
         harness, reason, away"
        )
    );
    let unknown = error(&changed("\"session.finished\"]", "\"session.done\"]"));
    assert!(
        unknown
            .starts_with("hooks[0].on: \"session.done\" is not an event: they are session.started"),
        "{unknown}"
    );
    assert!(
        error(&changed(
            "on = [\"session.needs_you\", \"session.finished\"]",
            "on = []"
        ))
        .starts_with("hooks[0].on must be a list of events")
    );
    let without = changed("on = [\"session.needs_you\", \"session.finished\"]\n", "");
    assert!(error(&without).starts_with("hooks[0].on is missing"));
    // Named twice, an event is one.
    let twice = changed(
        "\"session.finished\"]",
        "\"session.finished\", \"session.finished\"]",
    );
    assert_eq!(manifest(&twice).unwrap().hooks[0].on.len(), 2);
}

#[test]
fn a_hook_s_values_of_the_wrong_kind_are_refused_naming_their_key() {
    for (old, new, key) in [
        ("timeout = 20", "timeout = 0", "hooks[0].timeout must be"),
        ("timeout = 20", "timeout = 61", "hooks[0].timeout must be"),
        (
            "timeout = 20",
            "timeout = \"20\"",
            "hooks[0].timeout must be",
        ),
        (
            "away = true",
            "away = \"yes\"",
            "hooks[0].when.away must be true or false",
        ),
        (
            "reason = [\"question\", \"permission\"]",
            "reason = [\"form\"]",
            "hooks[0].when.reason must be",
        ),
        (
            "project = [\"muster\"]",
            "project = \"muster\"",
            "hooks[0].when.project must be",
        ),
        ("id = \"ping\"", "id = \"Ping\"", "hooks[0].id must be"),
        (
            "command = [\"./notify\", \"{project}\", \"{branch}\"]",
            "command = \"./notify\"",
            "hooks[0].command must be",
        ),
    ] {
        let error = error(&changed(old, new));
        assert!(error.starts_with(key), "{new}: {error}");
    }
    let twice = changed("id = \"log\"", "id = \"ping\"");
    assert!(error(&twice).starts_with("hooks[1].id: \"ping\" is another's too"));
}

#[test]
fn a_hook_is_told_names_and_ids_never_an_agent_s_conversation() {
    for word in ["{transcript}", "{resume}"] {
        let error = error(&changed("\"{branch}\"]", &format!("\"{word}\"]")));
        assert_eq!(
            error,
            format!(
                "hooks[0].command: {word} is not told to a hook, which is given names and ids \
                 only"
            )
        );
    }
    let unknown = error(&changed("\"{branch}\"]", "\"{prompt}\"]"));
    assert!(
        unknown.ends_with(
            "they are {project} {root} {workspace} {branch} {session} {harness} {ticket} {pr}"
        ),
        "{unknown}"
    );
}

#[test]
fn a_session_s_placeholder_is_refused_where_an_event_is_no_session_s() {
    // Both of ping's events are a session's.
    let session = changed("\"{branch}\"]", "\"{session}\", \"{harness}\"]");
    assert!(manifest(&session).is_ok());
    let error = error(&changed("\"{workspace}\"]", "\"{session}\"]"));
    assert_eq!(
        error,
        "hooks[1].command: {session} is a session's, and worktree.created is no session's"
    );
}

#[test]
fn a_filter_none_of_the_hook_s_events_could_carry_is_refused() {
    let log_by_harness = changed(
        "command = [\"./log\", \"{workspace}\"]",
        "command = [\"./log\", \"{workspace}\"]\nwhen = { harness = [\"claude\"] }",
    );
    assert_eq!(
        error(&log_by_harness),
        "hooks[1].when.harness: none of the hook's events has one to test; only a session's \
         events (session.*, ticket.assigned) do"
    );
    let finished_by_reason = changed(
        "on = [\"session.needs_you\", \"session.finished\"]",
        "on = [\"session.finished\"]",
    );
    assert_eq!(
        error(&finished_by_reason),
        "hooks[0].when.reason: none of the hook's events has one to test; only \
         session.needs_you do"
    );
}

#[test]
fn a_manifest_of_nothing_names_hooks_among_what_it_could_give() {
    let nothing = "id = \"p\"\nname = \"P\"\nversion = \"1\"\nmuster = \"0.3\"\n";
    assert_eq!(
        manifest(nothing).unwrap_err().to_string(),
        "a plugin gives at least one [[panels]], [[hooks]] or a [tickets]"
    );
}

//! End-to-end tests of plugins' hooks (`PRD-15`, `ARC-23`, `ARCHITECTURE.md`
//! §7a) against the real daemon: a linked fixture plugin's hooks write each
//! event they hear to a file, as a stand-in Claude Code's session starts,
//! comes to need the person, finishes and ends, and a worktree is made and
//! removed: each once, as filtered, away from Muster or not; nothing once
//! the plugin is off. `muster plugin hook` tries one on a sample; a reload
//! cuts a run still going after its grace; and the example notify-webhook
//! posts to a local stand-in for a webhook, so it cannot rot.

// Helpers outside `#[test]` functions are not covered by clippy.toml's test exemption.
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "end-to-end tests: a failed step is the test failing"
)]
#![expect(
    clippy::disallowed_methods,
    reason = "tests time themselves; ARC-03 binds Muster's own code"
)]

mod support;

use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};
use std::time::{Duration, Instant};

use muster_client::Connection;
use muster_core::protocol::{Command, Delta, Reply, ServerFrame, TermSize};
use muster_core::{SessionId, SessionStatus, WorkspaceId};
use serde_json::{Value, json};

use support::{Sandbox, expect_event};

/// Writes each event it hears as a line to `events` beside it: the hook,
/// the event's name, and the event as given on stdin.
const RECORD: &str = r#"#!/bin/sh
line=$(cat)
printf '%s %s %s\n' "$1" "$MUSTER_PLUGIN_EVENT" "$line" >> "$(dirname "$0")/events"
"#;

const RECORDER: &str = r#"id = "recorder"
name = "Recorder"
version = "0.1.0"
muster = "0.4.0"

[[hooks]]
id = "all"
on = ["session.started", "session.needs_you", "session.finished", "session.error",
      "session.ended", "worktree.created", "worktree.removed", "ticket.assigned"]
command = ["./record", "all"]

[[hooks]]
id = "away"
on = ["session.needs_you"]
command = ["./record", "away"]
[hooks.when]
away = true
reason = ["permission"]

[[hooks]]
id = "looking"
on = ["session.needs_you"]
command = ["./record", "looking"]
[hooks.when]
away = false
harness = ["claude"]

[[hooks]]
id = "elsewhere"
on = ["worktree.created"]
command = ["./record", "elsewhere"]
[hooks.when]
project = ["not-this-one"]

[[hooks]]
id = "say"
on = ["clash.found"]
command = ["./say", "{project}"]
"#;

/// A folder of the sandbox's holding a plugin: `manifest` and `programs`.
fn plugin_folder(
    sandbox: &Sandbox,
    name: &str,
    manifest: &str,
    programs: &[(&str, &str)],
) -> PathBuf {
    let folder = sandbox.root().join(name);
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("muster-plugin.toml"), manifest).unwrap();
    for (program, script) in programs {
        let path = folder.join(program);
        std::fs::write(&path, script).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    std::fs::canonicalize(folder).unwrap()
}

/// `muster plugin <args>` with no terminal.
fn plugin(sandbox: &Sandbox, args: &[&str]) -> Output {
    sandbox
        .command()
        .arg("plugin")
        .args(args)
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

fn said(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// `muster plugin <args>`, which must succeed.
fn ok(sandbox: &Sandbox, args: &[&str]) -> String {
    let output = plugin(sandbox, args);
    assert!(
        output.status.success(),
        "muster plugin {args:?}: {}",
        said(&output)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// The recorder, linked and approved.
fn recorder(sandbox: &Sandbox) -> PathBuf {
    let say = "#!/bin/sh\necho \"said for $1\"; echo \"and on stderr\" >&2; exit 3\n";
    let folder = plugin_folder(
        sandbox,
        "recorder",
        RECORDER,
        &[("record", RECORD), ("say", say)],
    );
    ok(sandbox, &["link", &folder.display().to_string(), "--yes"]);
    folder
}

/// The lines of `events` in `folder` so far.
fn heard(folder: &Path) -> Vec<String> {
    std::fs::read_to_string(folder.join("events"))
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}

/// How many lines of what was heard start `hook event`.
fn count(folder: &Path, hook: &str, event: &str) -> usize {
    let start = format!("{hook} {event} ");
    heard(folder)
        .iter()
        .filter(|line| line.starts_with(&start))
        .count()
}

/// Waits until `hook` has heard `event` `times` times, at most ten seconds;
/// the event as it was given the last time.
fn wait_heard(folder: &Path, hook: &str, event: &str, times: usize) -> Value {
    let deadline = Instant::now() + Duration::from_secs(10);
    while count(folder, hook, event) < times {
        assert!(
            Instant::now() < deadline,
            "{hook} never heard {event} {times} times: {:#?}",
            heard(folder)
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    let start = format!("{hook} {event} ");
    let line = heard(folder)
        .into_iter()
        .rfind(|line| line.starts_with(&start))
        .unwrap();
    serde_json::from_str(&line[start.len()..]).unwrap()
}

async fn project(connection: &mut Connection, sandbox: &Sandbox) -> muster_core::ProjectId {
    let root = sandbox.repository("app");
    let Reply::ProjectAdded { id } = connection
        .command(Command::AddProject { root, name: None })
        .await
        .unwrap()
    else {
        panic!("expected ProjectAdded");
    };
    id
}

async fn worktree(
    connection: &mut Connection,
    project: muster_core::ProjectId,
    branch: &str,
) -> WorkspaceId {
    let Reply::WorktreeCreated { id } = connection
        .command(Command::CreateWorktree {
            project,
            branch: Some(branch.to_owned()),
            on: None,
        })
        .await
        .unwrap()
    else {
        panic!("expected WorktreeCreated");
    };
    id
}

async fn start(connection: &mut Connection, workspace: WorkspaceId, harness: &str) -> SessionId {
    match connection
        .command(Command::StartSession {
            workspace,
            harness: harness.to_owned(),
            title: None,
            size: TermSize::default(),
            prompt: None,
        })
        .await
        .unwrap()
    {
        Reply::SessionStarted { id } => id,
        other => panic!("unexpected {other:?}"),
    }
}

/// Waits for `session` to come to `wanted`.
async fn status(connection: &mut Connection, session: SessionId, wanted: SessionStatus) {
    expect_event(connection, |frame| match frame {
        ServerFrame::Delta(Delta::SessionStatus {
            session: changed,
            status,
            ..
        }) if *changed == session && *status == wanted => Some(()),
        _ => None,
    })
    .await;
}

/// A payload shaped like Claude Code's.
fn payload(agent: &str, event: &str, extra: &Value) -> Value {
    let mut payload = json!({
        "session_id": agent,
        "hook_event_name": event,
        "cwd": "/work",
        "transcript_path": "/claude/projects/-work/x.jsonl",
    });
    if let (Some(payload), Some(extra)) = (payload.as_object_mut(), extra.as_object()) {
        payload.extend(extra.clone());
    }
    payload
}

#[tokio::test]
async fn a_hook_hears_each_event_of_an_agent_s_session_and_its_worktree_once_as_filtered() {
    let sandbox = Sandbox::new();
    let daemon = sandbox.start();
    let folder = recorder(&sandbox);
    let mut connection = daemon.connect().await;
    connection.subscribe().await.unwrap();
    let project = project(&mut connection, &sandbox).await;
    let workspace = worktree(&mut connection, project, "work").await;
    let created = wait_heard(&folder, "all", "worktree.created", 1);
    assert_eq!(created["project"]["name"], "app");
    assert_eq!(created["workspace"]["branch"], "work");
    assert!(
        created["workspace"]["folder"]
            .as_str()
            .unwrap()
            .ends_with("work")
    );

    // A shell is no agent's: it starts nothing a hook hears.
    start(&mut connection, workspace, "shell").await;
    let session = start(&mut connection, workspace, "claude").await;
    let started = wait_heard(&folder, "all", "session.started", 1);
    assert_eq!(started["session"]["id"], session.to_string());
    assert_eq!(started["session"]["harness"], "claude");
    let launch = sandbox.wait_for_launches(session, 1).remove(0);
    let agent = launch.value("--session-id").unwrap().to_owned();
    let fire = |event: &str, extra: Value| launch.fire(event, &payload(&agent, event, &extra));

    // Nobody looks at Muster: the hook for when the person is away hears
    // a permission asked.
    fire(
        "UserPromptSubmit",
        json!({ "prompt": "fix it with ghp_abcdefghijklmnopqrstuvwxyz0123" }),
    );
    status(&mut connection, session, SessionStatus::Running).await;
    fire("PermissionRequest", json!({ "tool_name": "Bash" }));
    let asked = wait_heard(&folder, "all", "session.needs_you", 1);
    assert_eq!(asked["session"]["status"], "needs_you");
    assert_eq!(asked["session"]["reason"], "permission");
    wait_heard(&folder, "away", "session.needs_you", 1);

    // The person looks: the hook for while they look hears a question,
    // and the one for a permission while away does not.
    connection.looking(true).await.unwrap();
    fire("PostToolUse", json!({ "tool_name": "Bash" }));
    status(&mut connection, session, SessionStatus::Running).await;
    fire("PreToolUse", json!({ "tool_name": "AskUserQuestion" }));
    let question = wait_heard(&folder, "looking", "session.needs_you", 1);
    assert_eq!(question["session"]["reason"], "question");
    wait_heard(&folder, "all", "session.needs_you", 2);

    // A finish is told once it has stayed one a moment.
    fire("PostToolUse", json!({ "tool_name": "AskUserQuestion" }));
    status(&mut connection, session, SessionStatus::Running).await;
    fire("Stop", json!({ "stop_hook_active": false }));
    status(&mut connection, session, SessionStatus::Finished).await;
    let finished_at = Instant::now();
    std::thread::sleep(Duration::from_millis(1_000));
    assert_eq!(
        count(&folder, "all", "session.finished"),
        0,
        "{:#?}",
        heard(&folder)
    );
    let finished = wait_heard(&folder, "all", "session.finished", 1);
    assert!(finished_at.elapsed() >= Duration::from_secs(2));
    assert_eq!(finished["session"]["status"], "finished");

    // Its program ends with an error of its own: an error, then its end.
    connection.attach(session, None).await.unwrap();
    connection.input(session, b"exit 3\n").await.unwrap();
    let failed = wait_heard(&folder, "all", "session.error", 1);
    assert_eq!(failed["session"]["success"], false);
    let ended = wait_heard(&folder, "all", "session.ended", 1);
    assert_eq!(ended["session"]["status"], "exited");
    // Another, stopped by the person, ends with no error.
    let stopped = start(&mut connection, workspace, "claude").await;
    wait_heard(&folder, "all", "session.started", 2);
    connection
        .command(Command::StopSession { session: stopped })
        .await
        .unwrap();
    wait_heard(&folder, "all", "session.ended", 2);
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(count(&folder, "all", "session.error"), 1);

    connection
        .command(Command::RemoveWorktree {
            workspace,
            force: true,
        })
        .await
        .unwrap();
    let removed = wait_heard(&folder, "all", "worktree.removed", 1);
    assert_eq!(removed["workspace"]["branch"], "work");

    // Each once; none for a project the filter does not name, and the
    // shell's start never.
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(count(&folder, "all", "session.started"), 2);
    assert_eq!(count(&folder, "all", "session.needs_you"), 2);
    assert_eq!(count(&folder, "away", "session.needs_you"), 1);
    assert_eq!(count(&folder, "looking", "session.needs_you"), 1);
    assert_eq!(count(&folder, "elsewhere", "worktree.created"), 0);
    // Names and ids only: the chat's name, as the UI shows it, named by
    // its first prompt with its credential hidden; never the prompt itself,
    // nor the agent's record of the conversation.
    assert_eq!(asked["session"]["name"], "fix it with [hidden by Muster]");
    let all = heard(&folder).join("\n");
    for never in ["ghp_", "\"prompt\"", "transcript", "/claude/projects"] {
        assert!(!all.contains(never), "{never} reached a hook: {all}");
    }
    // What each run came to is in the plugin's log.
    let log = ok(&sandbox, &["log", "recorder"]);
    assert!(
        log.contains("[hook all · worktree.created] exited 0"),
        "{log}"
    );
}

#[tokio::test]
async fn a_plugin_turned_off_hears_nothing_and_turned_on_hears_again() {
    let sandbox = Sandbox::new();
    let daemon = sandbox.start();
    let folder = recorder(&sandbox);
    let mut connection = daemon.connect().await;
    let project = project(&mut connection, &sandbox).await;
    ok(&sandbox, &["disable", "recorder"]);
    worktree(&mut connection, project, "quiet").await;
    std::thread::sleep(Duration::from_millis(1_000));
    assert_eq!(heard(&folder), Vec::<String>::new());
    ok(&sandbox, &["enable", "recorder", "--yes"]);
    worktree(&mut connection, project, "heard").await;
    let created = wait_heard(&folder, "all", "worktree.created", 1);
    assert_eq!(created["workspace"]["branch"], "heard");
    // A manifest changed since its approval turns it off, with why.
    let manifest = folder.join("muster-plugin.toml");
    let changed = std::fs::read_to_string(&manifest).unwrap() + "\n# changed\n";
    std::fs::write(&manifest, changed).unwrap();
    worktree(&mut connection, project, "changed").await;
    std::thread::sleep(Duration::from_millis(1_000));
    assert_eq!(count(&folder, "all", "worktree.created"), 1);
    let log = ok(&sandbox, &["log", "recorder"]);
    assert!(
        log.contains("not run: its manifest changed since you approved it"),
        "{log}"
    );
}

#[tokio::test]
async fn muster_plugin_hook_tries_a_plugin_s_hooks_on_a_sample_and_says_what_each_did() {
    let sandbox = Sandbox::new();
    let _daemon = sandbox.start();
    let folder = recorder(&sandbox);

    let tried = ok(&sandbox, &["hook", "recorder", "session.needs_you"]);
    // Every hook that runs for it, its filters aside.
    for hook in ["all", "away", "looking"] {
        assert!(
            tried.contains(&format!(
                "{hook} for a sample session.needs_you: exited 0 after"
            )),
            "{tried}"
        );
    }
    let sample = wait_heard(&folder, "away", "session.needs_you", 1);
    assert_eq!(sample["sample"], true);
    assert_eq!(sample["project"]["name"], "sample");

    // One that fails is said so, with what it said, and fails the command.
    let failed = plugin(&sandbox, &["hook", "recorder", "clash.found"]);
    assert!(!failed.status.success());
    let words = said(&failed);
    assert!(
        words.contains("say for a sample clash.found: exited 3 after"),
        "{words}"
    );
    // Its stdout and stderr, read together, in the order they came.
    for line in ["  said for sample\n", "  and on stderr\n"] {
        assert!(words.contains(line), "{line:?} in {words}");
    }
    assert!(words.contains("1 of 1 did not end well"), "{words}");

    let unknown = said(&plugin(&sandbox, &["hook", "recorder", "session.done"]));
    assert!(
        unknown.contains("session.done is not an event: they are session.started, "),
        "{unknown}"
    );
    let none = said(&plugin(
        &sandbox,
        &["hook", "recorder", "pull_request.changed"],
    ));
    assert!(
        none.contains("no hook of recorder's runs for pull_request.changed: all runs for"),
        "{none}"
    );
    ok(&sandbox, &["disable", "recorder"]);
    let off = said(&plugin(
        &sandbox,
        &["hook", "recorder", "session.needs_you"],
    ));
    assert!(off.contains("recorder is turned off"), "{off}");
}

#[tokio::test]
async fn a_finish_still_held_as_muster_reloads_is_told_once_by_the_daemon_that_takes_over() {
    let sandbox = Sandbox::new();
    let daemon = sandbox.start();
    let folder = recorder(&sandbox);
    let mut connection = daemon.connect().await;
    connection.subscribe().await.unwrap();
    let project = project(&mut connection, &sandbox).await;
    let workspace = worktree(&mut connection, project, "work").await;
    let session = start(&mut connection, workspace, "claude").await;
    let launch = sandbox.wait_for_launches(session, 1).remove(0);
    let agent = launch.value("--session-id").unwrap().to_owned();
    let fire = |event: &str, extra: Value| launch.fire(event, &payload(&agent, event, &extra));
    fire("UserPromptSubmit", json!({ "prompt": "fix it" }));
    status(&mut connection, session, SessionStatus::Running).await;
    fire("Stop", json!({ "stop_hook_active": false }));
    status(&mut connection, session, SessionStatus::Finished).await;

    // Reloaded within the finish's hold: the daemon taking over tells it
    // as its hold ends, and only once.
    let reloaded = sandbox.run(&["reload"]);
    assert!(reloaded.status.success(), "{}", said(&reloaded));
    let finished = wait_heard(&folder, "all", "session.finished", 1);
    assert_eq!(finished["session"]["id"], session.to_string());
    std::thread::sleep(Duration::from_secs(3));
    assert_eq!(
        count(&folder, "all", "session.finished"),
        1,
        "{:#?}",
        heard(&folder)
    );
}

#[tokio::test]
async fn a_reload_gives_a_hook_under_way_its_grace_then_cuts_it() {
    let sandbox = Sandbox::new();
    let manifest = "id = \"slow\"\nname = \"Slow\"\nversion = \"0.1.0\"\nmuster = \"0.4.0\"\n\n\
                    [[hooks]]\nid = \"slow\"\non = [\"worktree.created\"]\n\
                    command = [\"./slow\"]\ntimeout = 60\n";
    let slow = "#!/bin/sh\necho $$ > \"$(dirname \"$0\")/pid\"\nsleep 30\ntouch \"$(dirname \"$0\")/finished\"\n";
    let daemon = sandbox.start();
    let folder = plugin_folder(&sandbox, "slow", manifest, &[("slow", slow)]);
    ok(&sandbox, &["link", &folder.display().to_string(), "--yes"]);
    let mut connection = daemon.connect().await;
    let project = project(&mut connection, &sandbox).await;
    worktree(&mut connection, project, "work").await;
    let pid_file = folder.join("pid");
    let deadline = Instant::now() + Duration::from_secs(10);
    while !pid_file.exists() {
        assert!(Instant::now() < deadline, "the hook never started");
        std::thread::sleep(Duration::from_millis(50));
    }
    let pid: i32 = std::fs::read_to_string(&pid_file)
        .unwrap()
        .trim()
        .parse()
        .unwrap();

    let reloading = Instant::now();
    let reloaded = sandbox.run(&["reload"]);
    assert!(reloaded.status.success(), "{}", said(&reloaded));
    assert!(reloading.elapsed() >= Duration::from_secs(2));
    let gone = Instant::now() + Duration::from_secs(5);
    while nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None).is_ok() {
        assert!(Instant::now() < gone, "the hook outlived the reload");
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(!folder.join("finished").exists());
    let log = daemon.log();
    assert!(log.contains("cut a hook's run"), "{log}");
}

#[tokio::test]
async fn the_notify_webhook_example_posts_each_event_to_its_webhook() {
    let (address, posted) = support::http::serve(|_, _| (200, "ok".to_owned()));
    let sandbox = Sandbox::new().with_env("WEBHOOK_URL", &format!("{address}/services/T0/B0/x"));
    let example =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/plugins/notify-webhook");
    let example = std::fs::canonicalize(example).unwrap();
    let daemon = sandbox.start();
    ok(&sandbox, &["link", &example.display().to_string(), "--yes"]);

    let tried = ok(&sandbox, &["hook", "notify-webhook", "session.needs_you"]);
    assert!(
        tried.contains("notify for a sample session.needs_you: exited 0"),
        "{tried}"
    );
    let sample = posted.lock().unwrap().last().cloned().expect("it posted");
    assert_eq!(sample.method, "POST");
    assert_eq!(sample.target, "/services/T0/B0/x");
    assert_eq!(sample.header("content-type"), Some("application/json"));
    assert_eq!(
        sample.json(),
        json!({ "text": "sample · muster/issue-42-sample · A sample chat · has a question for you" })
    );

    // A real one: an agent asks for permission.
    let mut connection = daemon.connect().await;
    connection.subscribe().await.unwrap();
    let project = project(&mut connection, &sandbox).await;
    let workspace = worktree(&mut connection, project, "work").await;
    let session = start(&mut connection, workspace, "claude").await;
    let launch = sandbox.wait_for_launches(session, 1).remove(0);
    let agent = launch.value("--session-id").unwrap().to_owned();
    launch.fire(
        "UserPromptSubmit",
        &payload(&agent, "UserPromptSubmit", &json!({})),
    );
    status(&mut connection, session, SessionStatus::Running).await;
    let tool = json!({ "tool_name": "Bash" });
    launch.fire(
        "PermissionRequest",
        &payload(&agent, "PermissionRequest", &tool),
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    let text = loop {
        let last = posted
            .lock()
            .unwrap()
            .last()
            .map(support::http::Request::json);
        if let Some(text) = last
            .and_then(|body| body["text"].as_str().map(str::to_owned))
            .filter(|text| text.ends_with("needs your permission"))
        {
            break text;
        }
        assert!(
            Instant::now() < deadline,
            "nothing posted: {:?}",
            posted.lock().unwrap()
        );
        std::thread::sleep(Duration::from_millis(50));
    };
    assert!(text.starts_with("app · work · "), "{text}");
    // Its secret, the webhook's address, never in its log.
    let log = ok(&sandbox, &["log", "notify-webhook"]);
    assert!(log.contains("posted: app · work · "), "{log}");
    assert!(!log.contains("/services/T0/B0/x"), "{log}");
}

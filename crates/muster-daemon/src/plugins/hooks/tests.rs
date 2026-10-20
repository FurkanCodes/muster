use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;
use std::time::Duration;

use muster_core::plugins::{Event, EventKind, Hook, When};
use muster_core::protocol::plugins::{HookOutcome, HookRun};
use muster_core::time::Timestamp;
use tokio::sync::oneshot;

use super::*;

/// A plugin's folder: its manifest, and its programs, each a `sh` script.
struct Plugin {
    dir: tempfile::TempDir,
    sha256: String,
}

impl Plugin {
    fn new(programs: &[(&str, &str)]) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let manifest = "id = \"notify\"\n";
        std::fs::write(dir.path().join("muster-plugin.toml"), manifest).unwrap();
        for (name, script) in programs {
            let path = dir.path().join(name);
            std::fs::write(&path, format!("#!/bin/sh\n{script}\n")).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        Self {
            dir,
            sha256: crate::plugins::sha256(manifest.as_bytes()),
        }
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    /// A run of `command` for a finish, with a deadline of `timeout`
    /// seconds; and what it comes to.
    fn job(&self, command: &[&str], timeout: u64) -> (Job, oneshot::Receiver<HookRun>) {
        let (done, ran) = oneshot::channel();
        let job = Job {
            plugin: "notify".to_owned(),
            hook: Hook {
                id: "ping".to_owned(),
                on: vec![EventKind::SessionFinished],
                command: command.iter().map(|part| (*part).to_owned()).collect(),
                timeout,
                when: When::default(),
            },
            event: Event::sample(EventKind::SessionFinished, Timestamp::from_millis(7)),
            folder: self.path().to_owned(),
            sha256: self.sha256.clone(),
            env: vec!["HOME".to_owned()],
            secrets: vec!["WEBHOOK_URL".to_owned()],
            refused: None,
            done: Some(done),
        };
        (job, ran)
    }
}

fn hooks(limits: Limits) -> Hooks {
    let secrets = crate::secrets::Kept::default();
    secrets.0.lock().unwrap().insert(
        crate::secrets::plugin_account("notify", "WEBHOOK_URL"),
        crate::secrets::Token::new("https://hooks.example/s3cr3t-path"),
    );
    Hooks::new(
        Tasks::default(),
        crate::trackers::plugin::Plugins::new("muster".into()),
        Arc::new(secrets),
    )
    .with_limits(limits)
}

fn quick() -> Limits {
    Limits {
        stop_grace: Duration::from_millis(200),
        ..Limits::default()
    }
}

async fn outcome(ran: oneshot::Receiver<HookRun>) -> HookRun {
    tokio::time::timeout(Duration::from_secs(20), ran)
        .await
        .expect("the run ended")
        .expect("it was told")
}

/// Waits until `done` holds, at most five seconds.
async fn until(done: impl Fn() -> bool) {
    for _ in 0..500 {
        if done() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("it never came to be");
}

fn alive(pid_file: &Path) -> bool {
    let pid: i32 = std::fs::read_to_string(pid_file)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    nix::sys::signal::kill(Pid::from_raw(pid), None).is_ok()
}

#[tokio::test]
async fn a_hook_is_told_its_event_with_its_context_and_only_what_its_manifest_names() {
    let plugin = Plugin::new(&[(
        "notify",
        "cat > event.json; printf '%s\\n' \"$@\" > args; env > env; echo told",
    )]);
    let hooks = hooks(quick());
    let (job, ran) = plugin.job(&["./notify", "{project}", "{branch}"], 10);
    hooks.raise(vec![job]);
    let run = outcome(ran).await;
    assert_eq!(run.outcome, HookOutcome::Exited { code: 0 });
    assert_eq!(run.output, "told\n");
    let event: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(plugin.path().join("event.json")).unwrap())
            .unwrap();
    assert_eq!(event["event"], "session.finished");
    assert_eq!(event["at"], 7);
    assert_eq!(event["session"]["name"], "A sample chat");
    let args = std::fs::read_to_string(plugin.path().join("args")).unwrap();
    assert_eq!(args, "sample\nmuster/issue-42-sample\n");
    let env = std::fs::read_to_string(plugin.path().join("env")).unwrap();
    for given in [
        "MUSTER_PLUGIN_EVENT=session.finished",
        "MUSTER_PLUGIN_PROJECT=sample",
        "MUSTER_PLUGIN_TICKET=42",
        "MUSTER_PLUGIN_SESSION=ses_sample",
        "WEBHOOK_URL=https://hooks.example/s3cr3t-path",
        "PATH=",
        "HOME=",
    ] {
        assert!(
            env.lines().any(|line| line.starts_with(given)),
            "{given} in {env}"
        );
    }
    // Nothing of the daemon's own beyond the basics and what it names.
    for never in [
        "CARGO",
        "RUST",
        "MUSTER_PLUGIN_RESUME",
        "MUSTER_PLUGIN_TRANSCRIPT",
    ] {
        assert!(
            !env.lines().any(|line| line.starts_with(never)),
            "{never} in {env}"
        );
    }
    let log = hooks.logs.log("notify");
    assert!(
        log.starts_with("[hook ping · session.finished] exited 0"),
        "{log}"
    );
    assert!(log.ends_with("told\n"), "{log}");
}

#[tokio::test]
async fn a_hook_s_secrets_never_show_in_what_it_said() {
    let plugin = Plugin::new(&[("notify", "echo \"posting to $WEBHOOK_URL\"")]);
    let hooks = hooks(quick());
    let (job, ran) = plugin.job(&["./notify"], 10);
    hooks.raise(vec![job]);
    let run = outcome(ran).await;
    assert_eq!(run.output, "posting to [hidden by Muster]\n");
    assert!(!hooks.logs.log("notify").contains("s3cr3t"));
}

#[tokio::test]
async fn a_hook_past_its_deadline_is_stopped_with_everything_it_started() {
    let plugin = Plugin::new(&[(
        "notify",
        "sleep 30 & echo $! > child.pid; echo waiting; sleep 30",
    )]);
    let hooks = hooks(quick());
    // Long enough for a busy machine to start it and hear from it.
    let (job, ran) = plugin.job(&["./notify"], 3);
    hooks.raise(vec![job]);
    let run = outcome(ran).await;
    assert_eq!(run.outcome, HookOutcome::TimedOut { seconds: 3 });
    // What it said before is kept.
    assert_eq!(run.output, "waiting\n");
    let child = plugin.path().join("child.pid");
    until(|| !alive(&child)).await;
    assert!(
        hooks
            .logs
            .log("notify")
            .starts_with("[hook ping · session.finished] stopped at its 3 s deadline")
    );
}

#[tokio::test]
async fn a_hook_is_done_when_its_program_ends_and_what_it_left_running_goes() {
    // What it leaves behind holds its output open.
    let plugin = Plugin::new(&[("notify", "sleep 30 & echo $! > child.pid; echo done")]);
    let hooks = hooks(quick());
    let (job, ran) = plugin.job(&["./notify"], 10);
    hooks.raise(vec![job]);
    let run = outcome(ran).await;
    assert_eq!(run.outcome, HookOutcome::Exited { code: 0 });
    assert!(run.millis < 5_000, "{}", run.millis);
    until(|| !alive(&plugin.path().join("child.pid"))).await;
}

#[tokio::test]
async fn what_a_hook_says_is_kept_up_to_its_cap_and_the_rest_let_go() {
    let plugin = Plugin::new(&[("notify", "head -c 200000 /dev/zero | tr '\\000' a; exit 3")]);
    let hooks = hooks(Limits {
        output: 1024,
        ..quick()
    });
    let (job, ran) = plugin.job(&["./notify"], 10);
    hooks.raise(vec![job]);
    let run = outcome(ran).await;
    assert_eq!(run.outcome, HookOutcome::Exited { code: 3 });
    assert!(
        run.output.starts_with(&"a".repeat(1024)),
        "{}",
        run.output.len()
    );
    assert!(run.output.ends_with("[more than 1 KiB; the rest let go]\n"));
    assert!(run.output.len() < 1100);
}

/// A program that marks it started, then waits for `go`.
const WAITS: &str = "touch \"started.$$\"; while [ ! -f go ]; do sleep 0.05; done";

fn started(plugin: &Plugin) -> usize {
    std::fs::read_dir(plugin.path())
        .unwrap()
        .filter(|entry| {
            entry
                .as_ref()
                .is_ok_and(|entry| entry.file_name().to_string_lossy().starts_with("started."))
        })
        .count()
}

#[tokio::test]
async fn at_most_four_of_a_plugin_s_hooks_run_at_once_and_the_rest_wait_their_turn() {
    let plugin = Plugin::new(&[("notify", WAITS)]);
    let hooks = hooks(quick());
    let (jobs, runs): (Vec<_>, Vec<_>) = (0..6).map(|_| plugin.job(&["./notify"], 20)).unzip();
    hooks.raise(jobs);
    until(|| started(&plugin) == 4).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(
        (started(&plugin), hooks.running(), hooks.waiting()),
        (4, 4, 2)
    );
    std::fs::write(plugin.path().join("go"), "").unwrap();
    for ran in runs {
        assert_eq!(outcome(ran).await.outcome, HookOutcome::Exited { code: 0 });
    }
    assert_eq!(started(&plugin), 6);
    until(|| hooks.running() == 0).await;
}

#[tokio::test]
async fn a_full_queue_drops_its_oldest_saying_so() {
    let plugin = Plugin::new(&[("notify", WAITS)]);
    let hooks = hooks(Limits {
        running: 1,
        waiting: 2,
        ..quick()
    });
    let (jobs, mut runs): (Vec<_>, Vec<_>) = (0..4).map(|_| plugin.job(&["./notify"], 20)).unzip();
    hooks.raise(jobs);
    // The first runs; of the three after it, the oldest goes.
    let dropped = outcome(runs.remove(1)).await;
    assert_eq!(
        dropped.outcome,
        HookOutcome::NotRun {
            why: "dropped: 2 of its plugin's runs waited already".to_owned()
        }
    );
    assert!(
        hooks
            .logs
            .log("notify")
            .contains("not run: dropped: 2 of its plugin's runs")
    );
    std::fs::write(plugin.path().join("go"), "").unwrap();
    for ran in runs {
        assert_eq!(outcome(ran).await.outcome, HookOutcome::Exited { code: 0 });
    }
}

#[tokio::test]
async fn a_plugin_that_can_no_longer_run_has_its_runs_cut_and_dropped() {
    let plugin = Plugin::new(&[("notify", WAITS)]);
    let hooks = hooks(Limits {
        running: 1,
        ..quick()
    });
    let (first, running) = plugin.job(&["./notify"], 20);
    let (second, waiting) = plugin.job(&["./notify"], 20);
    hooks.raise(vec![first, second]);
    until(|| started(&plugin) == 1).await;
    // Another plugin's turning off leaves it be.
    hooks.publish(&["notify".to_owned()]);
    assert_eq!((hooks.running(), hooks.waiting()), (1, 1));
    hooks.publish(&["other".to_owned()]);
    assert_eq!(outcome(running).await.outcome, HookOutcome::Cut);
    assert_eq!(
        outcome(waiting).await.outcome,
        HookOutcome::NotRun {
            why: "dropped: its plugin can no longer run".to_owned()
        }
    );
    until(|| hooks.running() == 0).await;
    assert_eq!(started(&plugin), 1);
}

#[tokio::test]
async fn a_manifest_changed_since_its_approval_runs_nothing() {
    let plugin = Plugin::new(&[("notify", "touch ran")]);
    std::fs::write(
        plugin.path().join("muster-plugin.toml"),
        "id = \"notify\"\n# and a hook added\n",
    )
    .unwrap();
    let hooks = hooks(quick());
    let (job, ran) = plugin.job(&["./notify"], 10);
    hooks.raise(vec![job]);
    let run = outcome(ran).await;
    assert!(
        matches!(&run.outcome, HookOutcome::NotRun { why } if why.starts_with("its manifest changed")),
        "{:?}",
        run.outcome
    );
    assert!(!plugin.path().join("ran").exists());
}

#[tokio::test]
async fn a_hook_that_cannot_start_or_lacks_a_value_is_not_run_saying_why() {
    let plugin = Plugin::new(&[]);
    let hooks = hooks(quick());
    let (missing, ran) = plugin.job(&["./missing"], 10);
    hooks.raise(vec![missing]);
    let run = outcome(ran).await;
    assert!(
        matches!(&run.outcome, HookOutcome::NotRun { why } if why.starts_with("could not start")),
        "{:?}",
        run.outcome
    );
    let (mut no_pr, ran) = plugin.job(&["./notify", "{pr}"], 10);
    no_pr.event.pull_request = None;
    hooks.raise(vec![no_pr]);
    assert_eq!(
        outcome(ran).await.outcome,
        HookOutcome::NotRun {
            why: "{pr} has no value: its branch has no pull request".to_owned()
        }
    );
    let (mut refused, ran) = plugin.job(&["./notify"], 10);
    refused.refused = Some("it needs curl, which is not on PATH".to_owned());
    hooks.raise(vec![refused]);
    assert_eq!(
        outcome(ran).await.outcome,
        HookOutcome::NotRun {
            why: "it needs curl, which is not on PATH".to_owned()
        }
    );
}

#[tokio::test]
async fn stopping_gives_runs_a_grace_then_cuts_them_and_runs_nothing_more() {
    let plugin = Plugin::new(&[("notify", WAITS), ("quick", "sleep 0.1")]);
    let hooks = hooks(quick());
    let (waits, cut) = plugin.job(&["./notify"], 20);
    let (quick, ended) = plugin.job(&["./quick"], 20);
    hooks.raise(vec![waits, quick]);
    until(|| started(&plugin) == 1).await;
    hooks.cut(Duration::from_millis(500)).await;
    // The quick one ended within the grace; the other was cut.
    assert_eq!(
        outcome(ended).await.outcome,
        HookOutcome::Exited { code: 0 }
    );
    assert_eq!(outcome(cut).await.outcome, HookOutcome::Cut);
    let (later, ran) = plugin.job(&["./quick"], 20);
    hooks.raise(vec![later]);
    assert_eq!(
        outcome(ran).await.outcome,
        HookOutcome::NotRun {
            why: "Muster is stopping".to_owned()
        }
    );
    // A restart in place that did not happen runs them again.
    hooks.reopen();
    let (again, ran) = plugin.job(&["./quick"], 20);
    hooks.raise(vec![again]);
    assert_eq!(outcome(ran).await.outcome, HookOutcome::Exited { code: 0 });
}

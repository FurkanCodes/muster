//! End-to-end tests for `muster daemon --detach` on macOS (`PRD-09`): at the
//! Mac's own screen launchd starts the daemon, so it is in no terminal
//! app's job and quitting the app leaves it running; where launchd will not,
//! or over SSH, the daemon is started directly, as before. Elsewhere there
//! is no launchd, and they pass at once.
// Helpers outside `#[test]` functions are not covered by clippy.toml's test exemption.
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "end-to-end tests: a failed step is the test failing"
)]
#![expect(
    clippy::disallowed_methods,
    reason = "tests time themselves and start their own threads and tasks; ARC-03, ARC-13 and ARC-14 bind Muster's own code"
)]

mod support;

use support::wait;

use std::path::{Path, PathBuf};
use std::process::Stdio;

use muster_client::Connection;
use muster_core::protocol::{Command, Reply, TermSize};
use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;

use support::{Sandbox, answers_hello, daemon_pid, read_terminal};

/// launchd's own `launchctl`, whatever stands in for it on the sandbox's
/// `PATH`.
const LAUNCHCTL: &str = "/bin/launchctl";

/// Only a Mac has launchd.
const ON_A_MAC: bool = cfg!(target_os = "macos");

/// Stops the daemon answering on its socket when the test ends.
struct StopDaemon(PathBuf);

impl Drop for StopDaemon {
    fn drop(&mut self) {
        if let Some(pid) = daemon_pid(&self.0).and_then(|pid| i32::try_from(pid).ok()) {
            let _ = kill(Pid::from_raw(pid), Signal::SIGTERM);
        }
    }
}

/// Runs `muster daemon --detach` in `sandbox` with `env`, from a shell that
/// first runs `before`, as a person's terminal would; waits for the daemon
/// to answer and returns its pid and what the starter (and a daemon it
/// started itself, which writes where it does) said.
fn detach(sandbox: &Sandbox, before: &str, env: &[(&str, &str)]) -> (u32, String) {
    let muster = sandbox.command();
    let mut shell = std::process::Command::new("/bin/sh");
    shell
        .arg("-c")
        .arg(format!("{before}\nexec \"$0\" daemon --detach"))
        .arg(muster.get_program());
    for (key, value) in muster.get_envs() {
        match value {
            Some(value) => shell.env(key, value),
            None => shell.env_remove(key),
        };
    }
    let said = sandbox.root().join("starter.err");
    let status = shell
        .env_remove("SSH_CONNECTION")
        .env_remove("SSH_TTY")
        .envs(env.iter().copied())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(std::fs::File::create(&said).unwrap())
        .status()
        .unwrap();
    assert!(
        status.success(),
        "{}",
        std::fs::read_to_string(&said).unwrap()
    );
    let socket = sandbox.paths().socket();
    assert!(
        wait::within(wait::WAIT, || answers_hello(&socket).then_some(())).is_some(),
        "the daemon did not start; the starter said:\n{}\nits log:\n{}",
        std::fs::read_to_string(&said).unwrap(),
        std::fs::read_to_string(sandbox.paths().runtime_dir.join("daemon.log")).unwrap_or_default()
    );
    (
        daemon_pid(&socket).unwrap(),
        std::fs::read_to_string(&said).unwrap(),
    )
}

/// What launchd says of `pid`'s resource coalition: its name.
fn coalition_of(pid: u32) -> String {
    let out = std::process::Command::new(LAUNCHCTL)
        .args(["print", &format!("pid/{pid}")])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    let block = text
        .split("resource coalition = {")
        .nth(1)
        .unwrap_or_else(|| panic!("no resource coalition in:\n{text}"));
    block
        .lines()
        .find_map(|line| line.trim().strip_prefix("name = "))
        .unwrap_or_else(|| panic!("no coalition name in:\n{block}"))
        .to_owned()
}

fn parent_of(pid: u32) -> u32 {
    let out = std::process::Command::new("ps")
        .args(["-o", "ppid=", "-p", &pid.to_string()])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().parse().unwrap()
}

/// Whether this user has a screen session launchd can start jobs in.
fn at_the_screen() -> bool {
    let uid = nix::unistd::getuid().as_raw();
    std::process::Command::new(LAUNCHCTL)
        .args(["print", &format!("gui/{uid}")])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// A stand-in `launchctl` on the sandbox's `PATH` that records its calls
/// and answers with `exit`, saying `says`.
fn stand_in_launchctl(sandbox: &Sandbox, says: &str, exit: u8) -> PathBuf {
    let calls = sandbox.root().join("launchctl-calls");
    let script = format!(
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\necho '{says}' >&2\nexit {exit}\n",
        calls.display()
    );
    let path = sandbox.root().join("bin/launchctl");
    std::fs::write(&path, script).unwrap();
    std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    calls
}

/// What is left of launchd jobs in the runtime directory.
fn job_files(runtime: &Path) -> Vec<String> {
    std::fs::read_dir(runtime)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with("launch-"))
        .collect()
}

/// Opens a shell in the daemon's care and has it say `what`.
async fn ask_a_shell(sandbox: &Sandbox, what: &str) -> String {
    let root = sandbox.repository("app");
    let mut connection = Connection::connect(&sandbox.paths().socket(), "muster e2e")
        .await
        .unwrap();
    let Reply::ProjectAdded { id: project } = connection
        .command(Command::AddProject { root, name: None })
        .await
        .unwrap()
    else {
        panic!("expected ProjectAdded");
    };
    let Reply::WorktreeCreated { id: workspace } = connection
        .command(Command::CreateWorktree {
            project,
            branch: Some("work".to_owned()),
            on: None,
            named_for: None,
        })
        .await
        .unwrap()
    else {
        panic!("expected WorktreeCreated");
    };
    let Reply::SessionStarted { id: session } = connection
        .command(Command::StartSession {
            workspace,
            harness: "shell".to_owned(),
            title: None,
            size: TermSize::default(),
            prompt: None,
            started_by: None,
        })
        .await
        .unwrap()
    else {
        panic!("expected SessionStarted");
    };
    connection.attach(session, None).await.unwrap();
    connection
        .input(
            session,
            format!("echo \"said: {what} end-$((6*7))\"\n").as_bytes(),
        )
        .await
        .unwrap();
    let (text, _) = read_terminal(&mut connection, session, "end-42").await;
    let line = text
        .lines()
        .rfind(|line| line.contains("said: ") && line.contains("end-42"))
        .unwrap()
        .to_owned();
    line.split("said: ").nth(1).unwrap().to_owned()
}

#[tokio::test]
async fn at_the_screen_launchd_starts_the_daemon_in_no_terminal_app_s_job_prd_09() {
    if !ON_A_MAC || !at_the_screen() {
        eprintln!("no screen session to start launchd jobs in; nothing to test here");
        return;
    }
    let sandbox = Sandbox::new();
    let runtime = sandbox.paths().runtime_dir;
    let _stop = StopDaemon(sandbox.paths().socket());
    let (daemon, said) = detach(
        &sandbox,
        "umask 027; ulimit -n 4321",
        &[("MUSTER_E2E_MARK", "carried")],
    );
    assert_eq!(said, "", "the starter said why it did not use launchd");

    // Started by launchd's job, which has gone: the daemon is launchd's
    // child, in the coalition the job had, not the one this test is in.
    assert_eq!(parent_of(daemon), 1);
    let coalition = coalition_of(daemon);
    assert!(coalition.starts_with("muster.daemon."), "{coalition}");
    assert_ne!(coalition, coalition_of(std::process::id()));
    let uid = nix::unistd::getuid().as_raw();
    let job = std::process::Command::new(LAUNCHCTL)
        .args(["print", &format!("gui/{uid}/{coalition}")])
        .output()
        .unwrap();
    assert!(!job.status.success(), "the job is still loaded");
    assert_eq!(job_files(&runtime), Vec::<String>::new());

    // Its sessions have the starter's environment, umask and open-files
    // limit, as they would have started from the terminal.
    let answer = ask_a_shell(&sandbox, "$MUSTER_E2E_MARK $(umask) $(ulimit -n)").await;
    assert_eq!(answer.trim_end_matches(" end-42"), "carried 0027 4321");
}

#[tokio::test]
async fn where_launchd_will_not_take_the_job_the_daemon_starts_directly_and_says_why() {
    if !ON_A_MAC {
        return;
    }
    let sandbox = Sandbox::new();
    let _stop = StopDaemon(sandbox.paths().socket());
    let calls = stand_in_launchctl(&sandbox, "Bootstrap failed: 5: Input/output error", 5);
    let (daemon, said) = detach(&sandbox, ":", &[]);
    assert!(
        said.contains(
            "starting the daemon directly, not through launchd: launchctl would not take the job: \
             Bootstrap failed: 5: Input/output error"
        ),
        "{said}"
    );
    let calls = std::fs::read_to_string(calls).unwrap();
    assert!(calls.starts_with("bootstrap gui/"), "{calls}");
    assert_eq!(coalition_of(daemon), coalition_of(std::process::id()));
    assert_eq!(
        job_files(&sandbox.paths().runtime_dir),
        Vec::<String>::new()
    );
}

#[tokio::test]
async fn over_ssh_the_daemon_is_started_directly() {
    if !ON_A_MAC {
        return;
    }
    let sandbox = Sandbox::new();
    let _stop = StopDaemon(sandbox.paths().socket());
    let calls = stand_in_launchctl(&sandbox, "", 0);
    let (daemon, said) = detach(
        &sandbox,
        ":",
        &[("SSH_CONNECTION", "192.0.2.1 50000 192.0.2.2 22")],
    );
    assert!(!said.contains("launchd"), "{said}");
    assert!(!calls.exists(), "launchctl was asked over SSH");
    assert_eq!(coalition_of(daemon), coalition_of(std::process::id()));
    assert_ne!(parent_of(daemon), std::process::id());
}

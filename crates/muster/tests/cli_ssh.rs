//! End-to-end tests for machines of the person's own (`PRODUCT.md` §6b,
//! `ARC-27`): `muster machine add` and `muster list machines`, and `muster
//! ssh` asking a machine what it has, installing Muster there only when
//! told to (a copy of this one, checked), saying what agents need, and
//! running Muster's command there. A stand-in `ssh` on `PATH` runs what it
//! is given with `sh`, in a home of its own, as the machine.

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

use std::path::PathBuf;
use std::process::Output;
use std::time::{Duration, Instant};

use support::{BINARY, Daemon, Sandbox};

struct Setup {
    sandbox: Sandbox,
    _daemon: Daemon,
    home: PathBuf,
}

fn setup() -> Setup {
    let sandbox = Sandbox::new();
    let home = sandbox.fake_ssh();
    let daemon = sandbox.start();
    Setup {
        sandbox,
        _daemon: daemon,
        home,
    }
}

impl Setup {
    fn run(&self, args: &[&str]) -> Output {
        self.sandbox
            .command()
            .args(args)
            .env("FAKE_SSH_HOME", &self.home)
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap()
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[tokio::test]
async fn a_machine_is_saved_by_name_and_listed_prd_06() {
    let setup = setup();
    let saved = setup.run(&["machine", "add", "vm", "tester@vm.example"]);
    assert!(saved.status.success(), "{}", text(&saved.stderr));
    let listed = text(&setup.run(&["list", "machines"]).stdout);
    assert!(
        listed.contains("vm") && listed.contains("tester@vm.example"),
        "{listed}"
    );
    let file =
        std::fs::read_to_string(setup.sandbox.paths().config_dir.join("machines.toml")).unwrap();
    assert!(file.contains("target = \"tester@vm.example\""), "{file}");
    assert!(
        !setup
            .run(&["machine", "add", "vm", "-oProxyCommand=sh"])
            .status
            .success()
    );
    assert!(setup.run(&["machine", "remove", "vm"]).status.success());
    assert!(text(&setup.run(&["list", "machines"]).stdout).starts_with("No machines"));
}

#[tokio::test]
async fn ssh_installs_only_when_told_then_runs_muster_there_arc_27() {
    let setup = setup();
    setup.run(&["machine", "add", "vm", "tester@vm.example"]);

    // Nothing there, and no terminal to ask on: refused, nothing installed.
    let refused = setup.run(&["ssh", "vm", "status"]);
    assert!(!refused.status.success());
    assert!(
        text(&refused.stderr).contains("--install"),
        "{}",
        text(&refused.stderr)
    );
    assert!(!setup.home.join(".local/bin/muster").exists());

    let first = setup.run(&["ssh", "vm", "--install", "status"]);
    assert!(first.status.success(), "{}", text(&first.stderr));
    let said = text(&first.stderr);
    assert!(said.contains("Installed muster"), "{said}");
    assert!(said.contains("On vm ("), "{said}");
    assert!(
        text(&first.stdout).contains("The daemon is running"),
        "{}",
        text(&first.stdout)
    );
    assert_eq!(
        std::fs::read(setup.home.join(".local/bin/muster")).unwrap(),
        std::fs::read(BINARY).unwrap(),
        "a copy of this muster"
    );

    // There now: nothing more to install, and the command runs there.
    let again = setup.run(&["ssh", "vm", "list", "machines"]);
    assert!(again.status.success(), "{}", text(&again.stderr));
    assert!(!text(&again.stderr).contains("Installed"));
    let calls = std::fs::read_to_string(setup.sandbox.root().join("ssh-calls")).unwrap();
    assert!(
        calls
            .lines()
            .all(|call| call.contains("-- tester@vm.example")),
        "{calls}"
    );
}

#[tokio::test]
async fn on_a_machine_every_command_asks_its_muster_through_ssh_arc_27() {
    let setup = setup();
    let repository = setup.sandbox.repository("app");
    assert!(
        setup
            .run(&["project", "add", repository.to_str().unwrap()])
            .status
            .success()
    );
    setup.run(&["machine", "add", "vm", "tester@vm.example"]);
    let installed = setup.run(&["ssh", "vm", "--install", "--check"]);
    assert!(installed.status.success(), "{}", text(&installed.stderr));
    std::fs::remove_file(setup.sandbox.root().join("ssh-calls")).unwrap();

    // Through the bridge there, the machine's daemon answers.
    let projects = setup.run(&["--on", "vm", "list", "projects"]);
    assert!(projects.status.success(), "{}", text(&projects.stderr));
    assert!(
        text(&projects.stdout).contains("app"),
        "{}",
        text(&projects.stdout)
    );
    let made = setup.run(&["--on", "vm", "workspace", "add", "fix-login", "-p", "app"]);
    assert!(made.status.success(), "{}", text(&made.stderr));
    let listed = setup
        .sandbox
        .command()
        .args(["list", "workspaces", "--all"])
        .env("MUSTER_MACHINE", "vm")
        .env("FAKE_SSH_HOME", &setup.home)
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert!(
        text(&listed.stdout).contains("fix-login"),
        "{}{}",
        text(&listed.stdout),
        text(&listed.stderr)
    );
    // Each through ssh, past the machine's greeting; none from a terminal
    // here, so none may ask.
    let calls = std::fs::read_to_string(setup.sandbox.root().join("ssh-calls")).unwrap();
    assert_eq!(calls.lines().count(), 3, "each through ssh: {calls}");
    for call in calls.lines() {
        for option in [
            "BatchMode=yes",
            "ServerAliveInterval=5",
            "ConnectTimeout=10",
            "-C",
        ] {
            assert!(call.contains(option), "{option}: {call}");
        }
        assert!(call.contains("muster bridge"), "{call}");
    }

    // This machine's folder means nothing there: where is named.
    let unnamed = setup.run(&["--on", "vm", "claude", "Fix it"]);
    assert!(!unnamed.status.success());
    assert!(
        text(&unnamed.stderr).contains("--in WORKSPACE"),
        "{}",
        text(&unnamed.stderr)
    );
    // What runs only here is refused there.
    let here = setup.run(&["--on", "vm", "daemon"]);
    assert!(!here.status.success());
    assert!(
        text(&here.stderr).contains("this command runs here"),
        "{}",
        text(&here.stderr)
    );
}

#[tokio::test]
async fn an_ssh_that_cannot_says_why_and_what_to_do_arc_27() {
    let setup = setup();
    setup.run(&["machine", "add", "vm", "tester@vm.example"]);
    let fails = setup.sandbox.root().join("ssh-fails");
    std::fs::write(
        &fails,
        "tester@vm.example: Permission denied (publickey).\n",
    )
    .unwrap();
    let denied = setup.run(&["--on", "vm", "list", "projects"]);
    assert!(!denied.status.success());
    let said = text(&denied.stderr);
    assert!(said.contains("ssh-add"), "{said}");
    assert!(said.contains("muster ssh vm"), "{said}");
    std::fs::write(&fails, "Host key verification failed.\n").unwrap();
    let untrusted = text(&setup.run(&["--on", "vm", "list", "projects"]).stderr);
    assert!(untrusted.contains("accept it"), "{untrusted}");
    // No Muster there to answer: said, and how to put it there.
    std::fs::remove_file(&fails).unwrap();
    let missing = text(&setup.run(&["--on", "vm", "list", "projects"]).stderr);
    assert!(missing.contains("--install"), "{missing}");
}

#[tokio::test]
async fn the_person_s_forwarded_ssh_agent_reaches_sessions_there_arc_27() {
    let setup = setup();
    let repository = setup.sandbox.repository("app");
    assert!(
        setup
            .run(&["project", "add", repository.to_str().unwrap()])
            .status
            .success()
    );
    setup.run(&["machine", "add", "vm", "tester@vm.example"]);
    setup.run(&["ssh", "vm", "--install", "--check"]);
    setup.run(&["--on", "vm", "workspace", "add", "keys", "-p", "app"]);
    // As the person's ssh forwards their agent: a socket of theirs.
    let agent = setup.sandbox.root().join("agent.sock");
    let _listening = std::os::unix::net::UnixListener::bind(&agent).unwrap();
    let started = setup
        .sandbox
        .command()
        .args(["--on", "vm", "term", "new", "--bg", "--in", "keys"])
        .env("FAKE_SSH_HOME", &setup.home)
        .env("SSH_AUTH_SOCK", &agent)
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert!(started.status.success(), "{}", text(&started.stderr));
    let link = setup.sandbox.paths().runtime_dir.join("ssh-agent.sock");
    assert_eq!(std::fs::read_link(&link).unwrap(), agent);
    setup.run(&[
        "--on",
        "vm",
        "send",
        "keys/1",
        "echo",
        "agent=$SSH_AUTH_SOCK",
    ]);
    let deadline = Instant::now() + Duration::from_secs(10);
    let wanted = format!("agent={}", link.display());
    loop {
        let shown = text(&setup.run(&["--on", "vm", "logs", "keys/1"]).stdout);
        if shown.contains(&wanted) {
            break;
        }
        assert!(Instant::now() < deadline, "no {wanted} in:\n{shown}");
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[tokio::test]
async fn a_binary_the_person_names_is_the_one_put_there_arc_27() {
    let setup = setup();
    setup.run(&["machine", "add", "vm", "tester@vm.example"]);
    // As a build of a branch for the machine's system would be.
    let given = setup.sandbox.root().join("muster-for-linux");
    std::fs::write(&given, "#!/bin/sh\necho 'muster 0.5.6'\n").unwrap();
    let installed = setup
        .sandbox
        .command()
        .args(["ssh", "vm", "--install", "--check"])
        .env("FAKE_SSH_HOME", &setup.home)
        .env("MUSTER_REMOTE_BINARY", &given)
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert!(installed.status.success(), "{}", text(&installed.stderr));
    assert_eq!(
        std::fs::read(setup.home.join(".local/bin/muster")).unwrap(),
        std::fs::read(&given).unwrap()
    );
}

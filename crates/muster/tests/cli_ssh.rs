//! End-to-end tests for machines of the person's own (`PRODUCT.md` §6b,
//! `ARC-27`): `muster machine add` and `muster list machines`, and `muster
//! ssh` asking a machine what it has, installing Muster there only when
//! told to (a copy of this one, or the release for its system downloaded
//! here, each checked), saying what agents need, and
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

use sha2::Digest as _;

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

    // A command there runs as ssh runs one, with Muster's PATH there.
    let first = setup.run(&["ssh", "vm", "--install", "muster", "status"]);
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
    let again = setup.run(&["ssh", "vm", "muster", "list", "machines"]);
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

#[tokio::test]
async fn a_binary_that_does_not_run_there_replaces_nothing_arc_27() {
    let setup = setup();
    setup.run(&["machine", "add", "vm", "tester@vm.example"]);
    // As a build for another system would be, there.
    let given = setup.sandbox.root().join("muster-for-another-system");
    std::fs::write(&given, "#!/bin/sh\nexit 126\n").unwrap();
    let installed = setup
        .sandbox
        .command()
        .args(["ssh", "vm", "--install", "--check"])
        .env("FAKE_SSH_HOME", &setup.home)
        .env("MUSTER_REMOTE_BINARY", &given)
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert!(!installed.status.success());
    assert!(
        text(&installed.stderr).contains("does not run on that machine"),
        "{}",
        text(&installed.stderr)
    );
    assert!(!setup.home.join(".local/bin/muster").exists());
}

/// Serves `files` by path on a local port, as a release's downloads are;
/// its address.
fn serve(files: Vec<(String, Vec<u8>)>) -> String {
    use std::io::{BufRead as _, Write as _};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            let _ = reader.read_line(&mut line);
            let path = line
                .split_whitespace()
                .nth(1)
                .unwrap_or_default()
                .to_owned();
            loop {
                let mut header = String::new();
                if reader.read_line(&mut header).is_err() || header.trim().is_empty() {
                    break;
                }
            }
            let body = files
                .iter()
                .find(|(at, _)| *at == path)
                .map(|(_, body)| body);
            let status = if body.is_some() {
                "200 OK"
            } else {
                "404 Not Found"
            };
            let body = body.cloned().unwrap_or_default();
            let head = format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(&body);
        }
    });
    address
}

/// A system other than the one the tests run on, as `uname -sm` says it,
/// and its release's target: the machine needs a release, never a copy of
/// this muster.
fn other_system() -> (&'static str, &'static str) {
    if (std::env::consts::OS, std::env::consts::ARCH) == ("linux", "x86_64") {
        ("Linux aarch64", "aarch64-unknown-linux-gnu")
    } else {
        ("Linux x86_64", "x86_64-unknown-linux-gnu")
    }
}

/// A machine of [`other_system`], and a release of this version for it
/// served on a local port, its checksum file saying `checksum` (the right
/// one if `None`): the release's address and its program.
fn linux_release(setup: &Setup, checksum: Option<&str>) -> (String, Vec<u8>) {
    use std::os::unix::fs::PermissionsExt as _;
    let bin = setup.home.join(".local/bin");
    std::fs::create_dir_all(&bin).unwrap();
    let (system, target) = other_system();
    std::fs::write(bin.join("uname"), format!("#!/bin/sh\necho '{system}'\n")).unwrap();
    std::fs::set_permissions(bin.join("uname"), std::fs::Permissions::from_mode(0o755)).unwrap();
    let version = text(
        &setup
            .sandbox
            .command()
            .arg("--version")
            .output()
            .unwrap()
            .stdout,
    );
    let name = format!("muster-{target}");
    let name = name.as_str();
    let dir = setup.sandbox.root().join("release");
    std::fs::create_dir_all(dir.join(name)).unwrap();
    let program = format!("#!/bin/sh\necho '{}'\n", version.trim());
    std::fs::write(dir.join(name).join("muster"), &program).unwrap();
    let archive = dir.join(format!("{name}.tar.gz"));
    let packed = std::process::Command::new("tar")
        .arg("-czf")
        .arg(&archive)
        .arg("-C")
        .arg(&dir)
        .arg(name)
        .status()
        .unwrap();
    assert!(packed.success());
    let bytes = std::fs::read(&archive).unwrap();
    let right = muster_core::hex::encode(&sha2::Sha256::digest(&bytes));
    let sum = format!("{}  {name}.tar.gz\n", checksum.unwrap_or(&right));
    let base = serve(vec![
        (format!("/{name}.tar.gz"), bytes),
        (format!("/{name}.tar.gz.sha256"), sum.into_bytes()),
    ]);
    (base, program.into_bytes())
}

fn install_from(setup: &Setup, base: &str) -> Output {
    setup
        .sandbox
        .command()
        .args(["ssh", "vm", "--install", "--check"])
        .env("FAKE_SSH_HOME", &setup.home)
        .env("MUSTER_RELEASES", base)
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap()
}

#[tokio::test]
async fn another_system_s_release_is_downloaded_here_and_sent_over_ssh_arc_27() {
    let setup = setup();
    setup.run(&["machine", "add", "vm", "tester@vm.example"]);
    let (base, program) = linux_release(&setup, None);
    let installed = install_from(&setup, &base);
    assert!(installed.status.success(), "{}", text(&installed.stderr));
    assert_eq!(
        std::fs::read(setup.home.join(".local/bin/muster")).unwrap(),
        program,
        "the release's program, unpacked here"
    );
    // The machine downloaded nothing: no installer ran there.
    let calls = std::fs::read_to_string(setup.sandbox.root().join("ssh-calls")).unwrap();
    assert!(!calls.contains("install.sh"), "{calls}");
}

#[tokio::test]
async fn a_release_that_does_not_match_its_checksum_is_never_sent_arc_21() {
    let setup = setup();
    setup.run(&["machine", "add", "vm", "tester@vm.example"]);
    let (base, _) = linux_release(&setup, Some(&"0".repeat(64)));
    let refused = install_from(&setup, &base);
    assert!(!refused.status.success());
    assert!(
        text(&refused.stderr).contains("published checksum"),
        "{}",
        text(&refused.stderr)
    );
    assert!(!setup.home.join(".local/bin/muster").exists());
}

#[tokio::test]
async fn a_project_there_is_named_by_its_path_there_arc_27() {
    let setup = setup();
    setup.run(&["machine", "add", "vm", "tester@vm.example"]);
    assert!(
        setup
            .run(&["ssh", "vm", "--install", "--check"])
            .status
            .success()
    );
    let folder = setup.home.join("code-app");
    std::fs::create_dir_all(&folder).unwrap();
    // As the machine has it (a Mac's temporary folders are under a link).
    let folder = folder.canonicalize().unwrap();
    let path = folder.to_str().unwrap();
    let added = setup.run(&["--on", "vm", "project", "add", path]);
    assert!(added.status.success(), "{}", text(&added.stderr));

    // A path there is never looked for here: gone from this side, it is
    // still the project's, as typed.
    std::fs::remove_dir_all(&folder).unwrap();
    let removed = setup.run(&["--on", "vm", "project", "remove", &format!("{path}/")]);
    assert!(removed.status.success(), "{}", text(&removed.stderr));

    // Nor is a path made whole from this shell's folder.
    let relative = setup.run(&["--on", "vm", "project", "add", "code/app"]);
    assert!(!relative.status.success());
    assert!(
        text(&relative.stderr).contains("not a whole path on vm"),
        "{}",
        text(&relative.stderr)
    );
}

#[tokio::test]
async fn a_command_there_is_the_person_s_as_ssh_runs_one_arc_27() {
    let setup = setup();
    setup.run(&["machine", "add", "vm", "tester@vm.example"]);
    assert!(
        setup
            .run(&["ssh", "vm", "--install", "--check"])
            .status
            .success()
    );
    // Read by a shell there: a pipe is a pipe.
    let ran = setup.run(&["ssh", "vm", "echo", "quiet", "|", "tr", "a-z", "A-Z"]);
    assert!(ran.status.success(), "{}", text(&ran.stderr));
    assert!(text(&ran.stdout).contains("QUIET"), "{}", text(&ran.stdout));
}

#[tokio::test]
async fn a_machine_named_without_a_user_says_who_ssh_signs_in_as() {
    let setup = setup();
    setup.run(&["machine", "add", "vm", "vm.example"]);
    let checked = setup.run(&["ssh", "vm", "--install", "--check"]);
    let me = std::env::var("USER").unwrap();
    assert!(
        text(&checked.stderr).contains(&format!("as {me}, your name here")),
        "{}",
        text(&checked.stderr)
    );
    setup.run(&["machine", "add", "named", "tester@vm.example"]);
    let named = setup.run(&["ssh", "named", "--check"]);
    assert!(!text(&named.stderr).contains("your name here"));
}

/// Stands in for `curl` on the machine fetching Claude Code's installer:
/// records what it was asked for, and gives an installer that puts a
/// `claude` in `~/.local/bin`.
const MACHINE_CURL: &str = r##"#!/bin/sh
echo "$*" >> "$HOME/curl-calls"
echo 'mkdir -p "$HOME/.local/bin"; printf "#!/bin/sh\necho 2.1\n" > "$HOME/.local/bin/claude"; chmod +x "$HOME/.local/bin/claude"'
"##;

#[tokio::test]
async fn claude_code_is_installed_there_with_its_own_installer_prd_06() {
    use std::os::unix::fs::PermissionsExt as _;
    let setup = setup();
    setup.run(&["machine", "add", "vm", "tester@vm.example"]);
    // Not there, and no terminal to ask on: said how, nothing installed.
    let checked = setup.run(&["ssh", "vm", "--install", "--check"]);
    assert!(
        text(&checked.stderr).contains("`muster ssh vm --install-claude`"),
        "{}",
        text(&checked.stderr)
    );
    let claude = setup.home.join(".local/bin/claude");
    assert!(!claude.exists());

    let curl = setup.home.join(".local/bin/curl");
    std::fs::write(&curl, MACHINE_CURL).unwrap();
    std::fs::set_permissions(&curl, std::fs::Permissions::from_mode(0o755)).unwrap();
    let installed = setup.run(&["ssh", "vm", "--install-claude"]);
    let said = text(&installed.stderr);
    assert!(installed.status.success(), "{said}");
    assert!(claude.exists(), "its installer ran there");
    let asked = std::fs::read_to_string(setup.home.join("curl-calls")).unwrap();
    assert!(asked.contains("https://claude.ai/install.sh"), "{asked}");
    assert!(said.contains("`muster ssh vm claude auth login`"), "{said}");
}

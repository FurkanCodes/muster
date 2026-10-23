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

use support::{BINARY, Daemon, Sandbox};

/// The stand-in: options dropped, the host noted, the command run by `sh`
/// with `HOME` the machine's and only the system's programs, as on a
/// machine of its own.
const FAKE_SSH: &str = r#"#!/bin/sh
while [ $# -gt 0 ]; do
  case "$1" in
    -t|-T|-q) shift ;;
    -o) shift 2 ;;
    --) shift; break ;;
    *) break ;;
  esac
done
host="$1"; shift
echo "$host" >> "$FAKE_SSH_HOME/../ssh-hosts"
HOME="$FAKE_SSH_HOME" PATH="/usr/bin:/bin:/usr/sbin:/sbin" exec sh -c "$*"
"#;

struct Setup {
    sandbox: Sandbox,
    _daemon: Daemon,
    home: PathBuf,
}

fn setup() -> Setup {
    let sandbox = Sandbox::new();
    let home = sandbox.root().join("machine-home");
    std::fs::create_dir_all(&home).unwrap();
    let ssh = sandbox.root().join("bin").join("ssh");
    std::fs::write(&ssh, FAKE_SSH).unwrap();
    std::fs::set_permissions(&ssh, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
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
    let hosts = std::fs::read_to_string(setup.sandbox.root().join("ssh-hosts")).unwrap();
    assert!(
        hosts.lines().all(|host| host == "tester@vm.example"),
        "{hosts}"
    );
}

//! End-to-end tests for the UI on another machine's Muster (`PRODUCT.md`
//! §6b, `ARC-27`): `MUSTER_MACHINE=vm muster` shows that machine's state
//! through ssh, says it is that machine's, and connects again when the
//! link is lost, the machine's daemon started again by its bridge. A
//! stand-in `ssh` on `PATH` runs what it is given with `sh`, in a home of
//! its own, as the machine.

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
#[path = "tui/ui.rs"]
mod ui;

use std::time::{Duration, Instant};

use support::{Sandbox, daemon_pid};
use ui::{StopDaemon, Ui};

/// The stand-in machine: options dropped, the command run by `sh` with
/// `HOME` the machine's and only the system's programs.
const FAKE_SSH: &str = r#"#!/bin/sh
while [ $# -gt 0 ]; do
  case "$1" in
    -t|-T|-q) shift ;;
    -o) shift 2 ;;
    --) shift; break ;;
    *) break ;;
  esac
done
shift
HOME="$FAKE_SSH_HOME" PATH="/usr/bin:/bin:/usr/sbin:/sbin" exec sh -c "$*"
"#;

#[tokio::test]
async fn the_ui_shows_another_machine_s_muster_and_connects_again_arc_27() {
    let sandbox = Sandbox::new();
    let home = sandbox.root().join("machine-home");
    std::fs::create_dir_all(&home).unwrap();
    let ssh = sandbox.root().join("bin").join("ssh");
    std::fs::write(&ssh, FAKE_SSH).unwrap();
    std::fs::set_permissions(&ssh, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let daemon = sandbox.start();
    let repository = sandbox.repository("app");
    let home_text = home.to_str().unwrap();
    let run = |args: &[&str]| {
        let output = sandbox
            .command()
            .args(args)
            .env("FAKE_SSH_HOME", home_text)
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "`muster {}`: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    };
    run(&["project", "add", repository.to_str().unwrap()]);
    run(&["machine", "add", "vm", "tester@vm.example"]);
    run(&["ssh", "vm", "--install", "--check"]);

    let socket = sandbox.paths().socket();
    let _stop = StopDaemon(socket.clone());
    let ui = Ui::start_with(
        &sandbox,
        &repository,
        &[("MUSTER_MACHINE", "vm"), ("FAKE_SSH_HOME", home_text)],
    );
    ui.wait_for("on vm");
    ui.wait_for("app");

    // The machine's daemon goes; its bridge starts another as the UI
    // connects again, and the UI follows what changes there.
    let first = daemon.pid();
    daemon.stop();
    let deadline = Instant::now() + Duration::from_secs(20);
    while daemon_pid(&socket).is_none_or(|pid| pid == first) {
        assert!(Instant::now() < deadline, "no daemon started again there");
        std::thread::sleep(Duration::from_millis(100));
    }
    run(&["--on", "vm", "workspace", "add", "later", "-p", "app"]);
    ui.wait_for("later");
    ui.wait_for("on vm");
}

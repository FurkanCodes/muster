//! End-to-end tests of where each coding agent is installed and the version
//! it says it is (`Command::Agents`, protocol 45, for Settings › Agents):
//! from the real daemon, which looks on its own `PATH` for Muster's own
//! agents and those the person added, runs each with `--version` alone,
//! all at once, each within three seconds and killed past them, and says
//! only the version it read.

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

use std::path::Path;
use std::time::{Duration, Instant};

use muster_core::protocol::Reply;
use muster_core::protocol::agents::AgentFound;
use muster_core::protocol::{Command, PROTOCOL_VERSION};

use support::Sandbox;

/// Writes `script` as the program `name` in `dir`, ready to run.
fn program(dir: &Path, name: &str, script: &str) {
    let path = dir.join(name);
    std::fs::write(&path, script).unwrap();
    std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
}

/// Answers `--version` as Claude Code does, and anything else by failing.
const CLAUDE: &str = r#"#!/bin/sh
[ "$*" = --version ] || exit 3
echo '2.1.303 (Claude Code)'
"#;

/// Says nothing in time: it records its pid, then waits far past the
/// deadline.
const SLOW: &str = r#"#!/bin/sh
echo $$ > "$0.pid"
exec sleep 30
"#;

/// Says its version on standard error, as some do.
const ON_STDERR: &str = r"#!/bin/sh
echo 'GitHub Copilot CLI 0.0.339.' >&2
";

/// The person's own agent, which says more than its version.
const PI: &str = r"#!/bin/sh
printf 'pi version 1.2.0-beta.3+abc\nbuilt for darwin\n'
";

/// Whether the process `pid` still runs.
fn runs(pid: i32) -> bool {
    nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None).is_ok()
}

fn pid_of(bin: &Path, name: &str) -> i32 {
    std::fs::read_to_string(bin.join(format!("{name}.pid")))
        .unwrap()
        .trim()
        .parse()
        .unwrap()
}

#[tokio::test]
async fn each_agent_is_found_on_the_daemon_s_path_and_asked_its_version_at_once_within_its_deadline()
 {
    let sandbox = Sandbox::new();
    let bin = sandbox.root().join("agents-bin");
    std::fs::create_dir_all(&bin).unwrap();
    program(&bin, "claude", CLAUDE);
    program(&bin, "codex", SLOW);
    program(&bin, "opencode", SLOW);
    program(&bin, "copilot", ON_STDERR);
    program(&bin, "pi", PI);
    // Gemini CLI is not installed. A program Muster knows nothing of is
    // never run, however it is named.
    program(&bin, "agents", "#!/bin/sh\ntouch \"$0.ran\"\n");
    sandbox.own_settings("[agents.pi]\nlabel = \"Pi\"\nprogram = \"pi\"\n");
    // The daemon's own PATH: the agents above, and the system's for `sleep`.
    let sandbox = sandbox.with_env("PATH", &format!("{}:/usr/bin:/bin", bin.display()));
    let daemon = sandbox.start();
    let mut connection = daemon.connect().await;
    assert_eq!(connection.welcome().version, PROTOCOL_VERSION);

    let asked = Instant::now();
    let Reply::Agents(found) = connection.command(Command::Agents).await.unwrap() else {
        panic!("expected Agents");
    };
    let took = asked.elapsed();
    let at = |name: &str| Some(bin.join(name).to_string_lossy().into_owned());
    assert_eq!(
        found,
        [
            AgentFound {
                harness: "claude".to_owned(),
                path: at("claude"),
                version: Some("2.1.303".to_owned()),
            },
            AgentFound {
                harness: "codex".to_owned(),
                path: at("codex"),
                version: None,
            },
            AgentFound {
                harness: "gemini".to_owned(),
                path: None,
                version: None,
            },
            AgentFound {
                harness: "opencode".to_owned(),
                path: at("opencode"),
                version: None,
            },
            AgentFound {
                harness: "copilot".to_owned(),
                path: at("copilot"),
                version: Some("0.0.339".to_owned()),
            },
            AgentFound {
                harness: "pi".to_owned(),
                path: at("pi"),
                version: Some("1.2.0-beta.3+abc".to_owned()),
            },
        ]
    );
    // Both slow ones were waited for at once, three seconds and no more.
    assert!(took >= Duration::from_secs(3), "{took:?}");
    assert!(took < Duration::from_secs(5), "{took:?}");
    // And killed as the deadline passed.
    for slow in ["codex", "opencode"] {
        let pid = pid_of(&bin, slow);
        wait::until_async(&format!("{slow} to be killed"), async || !runs(pid)).await;
    }
    assert!(!bin.join("agents.ran").exists());
    // Asked again, they are asked again: nothing is kept or polled between.
    std::fs::remove_file(bin.join("codex")).unwrap();
    program(&bin, "codex", "#!/bin/sh\necho 'codex-cli 0.162.1'\n");
    let Reply::Agents(again) = connection.command(Command::Agents).await.unwrap() else {
        panic!("expected Agents");
    };
    assert_eq!(again[1].version.as_deref(), Some("0.162.1"));
}

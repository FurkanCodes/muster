//! End-to-end tests for workspaces from a shell (`PRODUCT.md` §6b):
//! `muster workspace add` and `ls`, `muster enter` taking a terminal tab
//! into a workspace's terminal, what is started there belonging to the
//! workspace and taking the terminal's place, Ctrl-Q going back and
//! leaving everything running, sessions named by number, and `muster
//! init`'s functions. The tab is a pseudo-terminal whose screen is parsed.

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

use std::io::{Read as _, Write as _};
use std::path::PathBuf;
use std::process::Output;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use muster_core::protocol::{Command, Reply, Snapshot};
use muster_core::{Session, SessionStatus, WorkspaceId};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};

use support::{BINARY, Daemon, Sandbox};

struct Setup {
    sandbox: Sandbox,
    daemon: Daemon,
    repository: PathBuf,
}

/// A daemon with one project, `app`; `muster` on the sessions' `PATH`, as
/// it is where it is installed.
async fn setup() -> Setup {
    let sandbox = Sandbox::new();
    let repository = sandbox.repository("app");
    std::os::unix::fs::symlink(BINARY, sandbox.root().join("bin").join("muster")).unwrap();
    let daemon = sandbox.start();
    let reply = daemon
        .connect()
        .await
        .command(Command::AddProject {
            root: repository.clone(),
            name: None,
        })
        .await
        .unwrap();
    assert!(matches!(reply, Reply::ProjectAdded { .. }), "{reply:?}");
    Setup {
        sandbox,
        daemon,
        repository,
    }
}

impl Setup {
    fn run(&self, args: &[&str]) -> Output {
        let output = self
            .sandbox
            .command()
            .current_dir(&self.repository)
            .args(args)
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "`muster {}`: {}",
            args.join(" "),
            text(&output.stderr)
        );
        output
    }

    async fn snapshot(&self) -> Snapshot {
        self.daemon.connect().await.subscribe().await.unwrap()
    }

    /// The workspace on `branch`, once it is there.
    async fn workspace(&self, branch: &str) -> WorkspaceId {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let found = self
                .snapshot()
                .await
                .workspaces
                .into_iter()
                .find(|workspace| workspace.kind.in_git() && workspace.kind.branch() == branch);
            if let Some(found) = found {
                return found.id;
            }
            assert!(Instant::now() < deadline, "no workspace on {branch}");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    /// The sessions in `workspace`, oldest first, once `count` are there.
    async fn sessions_in(&self, workspace: WorkspaceId, count: usize) -> Vec<Session> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let mut found: Vec<Session> = self
                .snapshot()
                .await
                .sessions
                .into_iter()
                .filter(|session| session.workspace == workspace)
                .collect();
            found.sort_by_key(|session| (session.created_at, session.id));
            if found.len() >= count {
                return found;
            }
            assert!(Instant::now() < deadline, "{found:?}");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    /// What `session` shows, until it shows `wanted`.
    fn until_shown(&self, session: &Session, wanted: &str) {
        let id = session.id.to_string();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let shown = text(&self.run(&["logs", &id]).stdout);
            if shown.contains(wanted) {
                return;
            }
            assert!(Instant::now() < deadline, "no {wanted:?} in:\n{shown}");
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// `muster` in a terminal tab of its own: a pseudo-terminal, its screen
/// parsed.
struct Tab {
    screen: Arc<Mutex<vt100::Parser>>,
    writer: Box<dyn std::io::Write + Send>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    _master: Box<dyn portable_pty::MasterPty + Send>,
}

impl Tab {
    fn open(setup: &Setup, args: &[&str]) -> Self {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: 30,
                cols: 100,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();
        let mut command = CommandBuilder::new(BINARY);
        command.args(args);
        command.cwd(&setup.repository);
        for (key, value) in setup.sandbox.command().get_envs() {
            match value {
                Some(value) => command.env(key, value),
                None => command.env_remove(key),
            }
        }
        command.env("TERM", "xterm-256color");
        let child = pair.slave.spawn_command(command).unwrap();
        drop(pair.slave);
        let screen = Arc::new(Mutex::new(vt100::Parser::new(30, 100, 0)));
        let parsed = screen.clone();
        let mut reader = pair.master.try_clone_reader().unwrap();
        std::thread::spawn(move || {
            let mut buffer = [0; 16 * 1024];
            while let Ok(read) = reader.read(&mut buffer) {
                if read == 0 {
                    break;
                }
                parsed.lock().unwrap().process(&buffer[..read]);
            }
        });
        let writer = pair.master.take_writer().unwrap();
        Self {
            screen,
            writer,
            child,
            _master: pair.master,
        }
    }

    fn type_in(&mut self, keys: &[u8]) {
        self.writer.write_all(keys).unwrap();
        self.writer.flush().unwrap();
    }

    /// Waits until the screen shows `wanted`.
    fn shows(&self, wanted: &str) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let contents = self.screen.lock().unwrap().screen().contents();
            if contents.contains(wanted) {
                return;
            }
            assert!(Instant::now() < deadline, "no {wanted:?} on:\n{contents}");
            std::thread::sleep(Duration::from_millis(30));
        }
    }

    /// Waits for `muster` to end, saying whether it succeeded.
    fn ends(&mut self) -> bool {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                return status.success();
            }
            assert!(Instant::now() < deadline, "the tab is still open");
            std::thread::sleep(Duration::from_millis(30));
        }
    }
}

#[tokio::test]
async fn a_workspace_is_made_by_name_and_listed_with_its_sessions_prd_6b() {
    let setup = setup().await;
    let made = text(&setup.run(&["workspace", "add", "fix-login"]).stdout);
    assert!(
        made.contains("Made workspace fix-login on muster/fix-login"),
        "{made}"
    );
    assert!(made.contains("muster enter fix-login"), "{made}");
    let workspace = setup.workspace("muster/fix-login").await;
    setup.run(&["term", "new", "--bg", "--in", "fix-login", "server"]);

    let listed = text(&setup.run(&["list", "workspaces"]).stdout);
    assert!(listed.contains("fix-login  1$"), "{listed}");
    let json: serde_json::Value =
        serde_json::from_slice(&setup.run(&["list", "workspaces", "--json"]).stdout).unwrap();
    let entry = json["workspaces"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["id"] == workspace.to_string())
        .unwrap();
    assert_eq!(entry["name"], "fix-login");
    assert_eq!(entry["kind"], "worktree");
    assert_eq!(entry["sessions"][0]["number"], 1);
    assert_eq!(entry["sessions"][0]["title"], "server");
}

#[tokio::test]
async fn sessions_are_named_by_their_number_in_their_workspace_prd_6b() {
    let setup = setup().await;
    setup.run(&["workspace", "add", "fix-login"]);
    let workspace = setup.workspace("muster/fix-login").await;
    setup.run(&["term", "new", "--bg", "--in", "fix-login"]);
    setup.run(&["term", "new", "--bg", "--in", "fix-login"]);
    let sessions = setup.sessions_in(workspace, 2).await;

    setup.run(&["rename", "fix-login/2", "dev", "server"]);
    let renamed = setup.sessions_in(workspace, 2).await;
    assert_eq!(renamed[1].title, "dev server");
    assert_eq!(renamed[0].title, sessions[0].title, "only the second");

    let listed = text(&setup.run(&["list"]).stdout);
    assert!(listed.contains("fix-login"), "{listed}");
    assert!(listed.contains(" 2 $ shell"), "{listed}");
    let terminals = text(&setup.run(&["list", "terminals"]).stdout);
    assert!(terminals.contains("2 terminals in app"), "{terminals}");
    let agents = text(&setup.run(&["list", "agents"]).stdout);
    assert!(agents.starts_with("No agents in app"), "{agents}");

    setup.run(&["close", "--yes", "fix-login/1"]);
    let left = setup.sessions_in(workspace, 1).await;
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].title, "dev server");

    setup.run(&["workspace", "rm", "--yes", "fix-login"]);
    assert!(
        !setup
            .snapshot()
            .await
            .workspaces
            .iter()
            .any(|w| w.id == workspace),
        "removed, its terminal stopped first"
    );
}

#[tokio::test]
async fn entering_a_workspace_shows_its_terminal_in_the_tab_and_ctrl_q_leaves_it_running() {
    let setup = setup().await;
    setup.run(&["workspace", "add", "fix-login"]);
    let workspace = setup.workspace("muster/fix-login").await;
    let mut tab = Tab::open(&setup, &["enter", "fix-login"]);
    let terminal = setup.sessions_in(workspace, 1).await.remove(0);
    assert!(terminal.kind().is_shell());
    tab.type_in(b"echo in-the-$((20+1))st\r");
    tab.shows("in-the-21st");
    setup.until_shown(&terminal, "in-the-21st");

    tab.type_in(b"\x11");
    assert!(tab.ends());
    tab.shows("it keeps running");
    let snapshot = setup.snapshot().await;
    let still = snapshot
        .sessions
        .iter()
        .find(|session| session.id == terminal.id)
        .unwrap();
    assert!(!still.status.is_exited(), "{:?}", still.status);

    // Entering again goes back to the same terminal.
    let mut again = Tab::open(&setup, &["enter", "fix-login"]);
    again.type_in(b"echo again-$((1+1))\r");
    setup.until_shown(&terminal, "again-2");
    assert_eq!(setup.sessions_in(workspace, 1).await.len(), 1);
    // Another terminal from inside: shown here, Ctrl-Q back to the first.
    again.type_in(b"muster term new\r");
    let both = setup.sessions_in(workspace, 2).await;
    assert!(both[1].kind().is_shell());
    again.type_in(b"echo second-$((1+1))\r");
    setup.until_shown(&both[1], "second-2");
    again.type_in(b"\x11echo first-$((1+2))\r");
    setup.until_shown(&terminal, "first-3");
    again.type_in(b"\x11");
    assert!(again.ends());
}

#[tokio::test]
async fn an_agent_started_in_a_workspace_s_terminal_belongs_to_it_and_takes_the_tab() {
    let setup = setup().await;
    setup.run(&["workspace", "add", "fix-login"]);
    let workspace = setup.workspace("muster/fix-login").await;
    let mut tab = Tab::open(&setup, &["enter", "fix-login"]);
    let terminal = setup.sessions_in(workspace, 1).await.remove(0);
    tab.type_in(b"echo ready-$((1+1))\r");
    tab.shows("ready-2");

    tab.type_in(b"muster claude Fix the redirect\r");
    let sessions = setup.sessions_in(workspace, 2).await;
    let agent = &sessions[1];
    assert_eq!(agent.harness, "claude");
    assert_eq!(agent.workspace, workspace, "in the terminal's workspace");
    assert_eq!(
        setup.snapshot().await.workspaces.len(),
        2,
        "no worktree made for it"
    );
    setup.sandbox.wait_for_launches(agent.id, 1);
    // The tab shows the agent now: what is typed reaches it (the stand-in
    // is a shell once started).
    tab.type_in(b"echo to-the-agent-$((3+4))\r");
    setup.until_shown(agent, "to-the-agent-7");

    // Ctrl-Q goes back to the terminal; the agent works on.
    tab.type_in(b"\x11");
    tab.type_in(b"echo back-$((2+2))\r");
    setup.until_shown(&terminal, "back-4");
    let snapshot = setup.snapshot().await;
    let still = snapshot
        .sessions
        .iter()
        .find(|session| session.id == agent.id)
        .unwrap();
    assert_eq!(still.status, SessionStatus::Running);

    // And again, to the person's own shell.
    tab.type_in(b"\x11");
    assert!(tab.ends());
}

#[tokio::test]
async fn init_s_functions_run_a_bare_agent_in_its_terminal_prd_13_arc_28() {
    let setup = setup().await;
    let printed = text(&setup.run(&["init", "bash"]).stdout);
    assert!(printed.contains("function claude {"), "{printed}");
    let script = setup.sandbox.root().join("muster-init.sh");
    std::fs::write(&script, &printed).unwrap();

    setup.run(&["workspace", "add", "fix-login"]);
    let workspace = setup.workspace("muster/fix-login").await;
    let mut tab = Tab::open(&setup, &["enter", "fix-login"]);
    setup.sessions_in(workspace, 1).await;
    // Read by bash, whose functions they are, as its rc file would.
    tab.type_in(b"exec bash --norc --noprofile\r");
    let source = format!(". '{}'; echo sourced-$((5+5))\r", script.display());
    tab.type_in(source.as_bytes());
    tab.shows("sourced-10");
    // `claude` runs in this terminal, its chat named by its prompt: no
    // other session.
    tab.type_in(b"claude Fix it\r");
    let deadline = Instant::now() + Duration::from_secs(10);
    let terminal = loop {
        let sessions = setup.sessions_in(workspace, 1).await;
        assert_eq!(sessions.len(), 1, "{sessions:?}");
        let terminal = sessions.into_iter().next().unwrap();
        let claude = terminal.agent.as_ref().map(|agent| agent.harness.as_str());
        if claude == Some("claude") && terminal.title == "Fix it" {
            break terminal;
        }
        assert!(Instant::now() < deadline, "{terminal:?}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    assert_eq!(terminal.harness, "shell");
    // It ends (the stand-in is a shell by now): the terminal again.
    tab.type_in(b"exit\r");
    let deadline = Instant::now() + Duration::from_secs(10);
    while setup.sessions_in(workspace, 1).await[0].agent.is_some() {
        assert!(Instant::now() < deadline, "the agent never left");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    tab.type_in(b"\x11");
    assert!(tab.ends());
}

#[tokio::test]
async fn keys_are_pressed_by_name_and_a_wait_is_for_text_prd_6b() {
    let setup = setup().await;
    setup.run(&["workspace", "add", "fix-login"]);
    let workspace = setup.workspace("muster/fix-login").await;
    setup.run(&["term", "new", "--bg", "--in", "fix-login", "server"]);
    setup.sessions_in(workspace, 1).await;

    setup.run(&["send", "fix-login/1", "sleep", "30"]);
    setup.run(&["keys", "fix-login/1", "ctrl+c"]);
    setup.run(&[
        "keys",
        "fix-login/1",
        "e",
        "c",
        "h",
        "o",
        "space",
        "o",
        "k",
        "-",
        "4",
        "2",
        "enter",
    ]);
    let waited = setup.run(&["wait", "fix-login/1", "--for", "ok-42", "--timeout", "10s"]);
    assert!(text(&waited.stdout).contains("ok-42"));

    let late = setup
        .sandbox
        .command()
        .current_dir(&setup.repository)
        .args([
            "wait",
            "fix-login/1",
            "--for",
            "never-shown",
            "--timeout",
            "1s",
        ])
        .output()
        .unwrap();
    assert_eq!(late.status.code(), Some(124));

    let refused = setup
        .sandbox
        .command()
        .current_dir(&setup.repository)
        .args(["keys", "fix-login/1", "hello"])
        .output()
        .unwrap();
    assert!(!refused.status.success(), "no key is called hello");
}

#[tokio::test]
async fn a_terminal_moves_to_another_workspace_and_an_agent_does_not_prd_6b() {
    let setup = setup().await;
    setup.run(&["workspace", "add", "fix-login"]);
    setup.run(&["workspace", "add", "api-cache"]);
    let (from, to) = (
        setup.workspace("muster/fix-login").await,
        setup.workspace("muster/api-cache").await,
    );
    setup.run(&["term", "new", "--bg", "--in", "fix-login", "server"]);
    setup.sessions_in(from, 1).await;
    let moved = text(
        &setup
            .run(&["move", "fix-login/1", "--to", "api-cache"])
            .stdout,
    );
    assert!(
        moved.contains("Moved it to api-cache (api-cache/1)"),
        "{moved}"
    );
    let there = setup.sessions_in(to, 1).await;
    assert_eq!(there[0].title, "server");
    let snapshot = setup.snapshot().await;
    assert!(
        !snapshot
            .sessions
            .iter()
            .any(|session| session.workspace == from),
        "the old one is closed"
    );

    setup.run(&["claude", "--in", "muster/fix-login", "--json", "Fix it"]);
    let refused = setup
        .sandbox
        .command()
        .current_dir(&setup.repository)
        .args(["move", "fix-login/1", "--to", "api-cache"])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    assert!(
        text(&refused.stderr).contains("is an agent"),
        "{}",
        text(&refused.stderr)
    );
}

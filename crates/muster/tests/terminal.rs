//! End-to-end tests for a session's terminal behaving as a real one: a
//! program asking its terminal's colours is told the person's.

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

use muster_client::Connection;
use muster_core::SessionId;
use muster_core::colours::{Rgb, TerminalColours};
use muster_core::protocol::{Command, Reply, TermSize};

use support::{Daemon, Sandbox, read_terminal};

/// A daemon with one project, and a shell attached in its checkout.
async fn shell(sandbox: &Sandbox) -> (Daemon, Connection, SessionId) {
    started(sandbox, "shell").await
}

/// A daemon with one project, and a session of `harness` attached in its
/// checkout: the stand-in for an agent behaves as a shell.
async fn started(sandbox: &Sandbox, harness: &str) -> (Daemon, Connection, SessionId) {
    let root = sandbox.repository("app");
    let daemon = sandbox.start();
    let mut connection = daemon.connect().await;
    let Reply::ProjectAdded { id: project } = connection
        .command(Command::AddProject { root, name: None })
        .await
        .unwrap()
    else {
        panic!("expected ProjectAdded");
    };
    let snapshot = connection.subscribe().await.unwrap();
    let workspace = snapshot
        .workspaces
        .iter()
        .find(|found| found.project == project)
        .map(|found| found.id)
        .expect("the project's checkout");
    let Reply::SessionStarted { id: session } = connection
        .command(Command::StartSession {
            workspace,
            harness: harness.to_owned(),
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
    (daemon, connection, session)
}

/// Has the shell ask its terminal `question` and print the `count` bytes
/// of the answer, ESC as `E` and a backslash as `B`, between `=` marks.
async fn asked(
    connection: &mut Connection,
    session: SessionId,
    question: &str,
    count: usize,
) -> String {
    let probe = format!(
        "stty -echo -icanon min 1; printf '{question}'; \
         r=$(dd bs=1 count={count} 2>/dev/null | tr '\\033\\\\' 'EB'); stty echo icanon; \
         echo \"answer=$r=$((2*21))\"\n"
    );
    connection.input(session, probe.as_bytes()).await.unwrap();
    let (text, _) = read_terminal(connection, session, "=42").await;
    text.rsplit("answer=")
        .next()
        .and_then(|rest| rest.split('=').next())
        .unwrap_or_default()
        .to_owned()
}

#[tokio::test]
async fn a_program_asking_its_terminal_s_colours_is_told_the_person_s() {
    let sandbox = Sandbox::new();
    let (_daemon, mut connection, session) = shell(&sandbox).await;
    // Before any UI has said them, a question about them goes unanswered,
    // as before: the answer read is the device attributes asked after it.
    let before = asked(&mut connection, session, "\\033]11;?\\007\\033[c", 5).await;
    assert_eq!(before, "E[?6c");

    connection
        .colours(TerminalColours {
            foreground: Some(Rgb {
                red: 0xd0d0,
                green: 0xd0d0,
                blue: 0xd0d0,
            }),
            background: Some(Rgb {
                red: 0x1e1e,
                green: 0x1e1e,
                blue: 0x2e2e,
            }),
            palette: vec![
                None,
                Some(Rgb {
                    red: 0xcdcd,
                    green: 0,
                    blue: 0,
                }),
            ],
        })
        .await
        .unwrap();
    // Ended as asked: BEL here, ST there.
    let background = asked(&mut connection, session, "\\033]11;?\\007", 24).await;
    assert_eq!(background, "E]11;rgb:1e1e/1e1e/2e2e\x07");
    let red = asked(&mut connection, session, "\\033]4;1;?\\033\\\\", 26).await;
    assert_eq!(red, "E]4;1;rgb:cdcd/0000/0000EB");
}

#[tokio::test]
async fn a_session_is_told_its_terminal_is_muster_whatever_the_daemon_started_in() {
    // A daemon started from iTerm2, inside tmux.
    let sandbox = Sandbox::new()
        .with_env("TERM_PROGRAM", "iTerm.app")
        .with_env("TERM_PROGRAM_VERSION", "3.6.6")
        .with_env("LC_TERMINAL", "iTerm2")
        .with_env("ITERM_SESSION_ID", "w0t0p0:6A2B")
        .with_env("TMUX", "/tmp/tmux-501/default,4242,0")
        .with_env("TMUX_PANE", "%3");
    let (_daemon, mut connection, session) = shell(&sandbox).await;
    connection
        .input(
            session,
            b"echo \"is-$TERM_PROGRAM-$TERM_PROGRAM_VERSION $COLORTERM ${LC_TERMINAL:-none} \
              ${ITERM_SESSION_ID:-none} ${TMUX:-none} ${TMUX_PANE:-none}=$((6*7))\"\n",
        )
        .await
        .unwrap();
    let (text, _) = read_terminal(&mut connection, session, "=42").await;
    let wanted = format!(
        "is-muster-{} truecolor none none none none=42",
        env!("CARGO_PKG_VERSION")
    );
    assert!(text.contains(&wanted), "{text:?}");
}

/// Says the person's keys are in `session`, or in none, and waits until
/// the daemon has taken it in: it reads a client's frames in order.
async fn keys(connection: &mut Connection, session: Option<SessionId>) {
    connection.keys(session).await.unwrap();
    connection.command(Command::Ping).await.unwrap();
}

#[tokio::test]
async fn a_program_that_asked_is_told_as_the_person_s_keys_come_and_go_in_any_client() {
    let sandbox = Sandbox::new();
    let (daemon, mut watching, session) = shell(&sandbox).await;
    // Asks to be told of focus, then prints the 9 bytes it is sent.
    let probe = "stty -echo -icanon min 1; printf '\\033[?1004h'; echo ready-$((1+2)); \
                 r=$(dd bs=1 count=9 2>/dev/null | tr '\\033' 'E'); stty echo icanon; \
                 echo \"focus=$r=$((2*21))\"\n";
    watching.input(session, probe.as_bytes()).await.unwrap();
    read_terminal(&mut watching, session, "ready-3").await;

    let mut one = daemon.connect().await;
    let mut two = daemon.connect().await;
    // In: the first client to have the keys says so.
    keys(&mut one, Some(session)).await;
    // A second has them too, and the first lets go: still in focus.
    keys(&mut two, Some(session)).await;
    keys(&mut one, None).await;
    // Out, as the last one goes, however it goes.
    drop(two);
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    // And in again.
    keys(&mut one, Some(session)).await;

    let (text, _) = read_terminal(&mut watching, session, "=42").await;
    let told = text
        .rsplit("focus=")
        .next()
        .and_then(|rest| rest.split('=').next())
        .unwrap_or_default();
    assert_eq!(told, "E[IE[OE[I", "{text:?}");
}

#[tokio::test]
async fn claude_code_is_told_it_is_in_focus_and_never_out_of_it() {
    let sandbox = Sandbox::new();
    let (daemon, mut watching, session) = started(&sandbox, "claude").await;
    // Asks to be told of focus, then prints the 4 bytes it is sent.
    let probe = "stty -echo -icanon min 1; printf '\\033[?1004h'; echo ready-$((1+2)); \
                 r=$(dd bs=1 count=4 2>/dev/null | tr '\\033' 'E'); stty echo icanon; \
                 echo \"focus=$r=$((2*21))\"\n";
    watching.input(session, probe.as_bytes()).await.unwrap();
    read_terminal(&mut watching, session, "ready-3").await;

    let mut one = daemon.connect().await;
    // In, as a client has the keys; never out, however it lets go of them,
    // so it writes its suggestion as a turn ends wherever the person is.
    keys(&mut one, Some(session)).await;
    keys(&mut one, None).await;
    keys(&mut one, Some(session)).await;
    drop(one);
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    watching.input(session, b"x").await.unwrap();

    let (text, _) = read_terminal(&mut watching, session, "=42").await;
    let told = text
        .rsplit("focus=")
        .next()
        .and_then(|rest| rest.split('=').next())
        .unwrap_or_default();
    assert_eq!(told, "E[Ix", "{text:?}");
}

#[tokio::test]
async fn a_program_sizing_pictures_is_told_its_terminal_s_size_in_pixels() {
    let sandbox = Sandbox::new();
    let (_daemon, mut connection, session) = shell(&sandbox).await;
    let size = TermSize { cols: 80, rows: 24 };
    let pixels = muster_core::protocol::PixelSize {
        width: 80 * 9,
        height: 24 * 18,
    };
    connection
        .resize_in_pixels(session, size, Some(pixels))
        .await
        .unwrap();
    // The text area's pixels, then a cell's (XTWINOPS 14 and 16).
    let told = asked(&mut connection, session, "\\033[14t\\033[16t", 21).await;
    assert_eq!(told, "E[4;432;720tE[6;18;9t");
}

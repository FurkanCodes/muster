//! The link to another machine's Muster (`--on`, `ARC-27`): `ssh` runs
//! `muster bridge` there, which joins it to that machine's daemon (started
//! where none runs) and says it is ready on a line of its own before any
//! frame, so what a login prints never reaches the protocol. The client
//! reads past that line, then speaks the daemon's own protocol across ssh's
//! standard input and output, untouched. ssh's complaints are kept and
//! said with what to do, never written over the UI.
//!
//! The bridge also gives the machine's sessions the person's ssh agent,
//! where their own ssh forwards it: a link of Muster's (`ssh-agent.sock`)
//! to the socket forwarded now, which the daemon names to every session
//! as `SSH_AUTH_SOCK` and which the next connection points at its own.

use std::io::Cursor;
use std::os::unix::fs::{FileTypeExt as _, MetadataExt as _};
use std::path::Path;
use std::process::{ExitCode, Stdio};
use std::time::Duration;

use muster_client::{ClientError, Connection};
use muster_core::Paths;
use muster_core::machines::{AGENT_LINK, BRIDGE_READY};
use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWriteExt as _};

use super::Remote;
use super::args::{How, REMOTE_PATH, control_dir, hint, quoted, ssh_args};

/// How much a login may print before the bridge says it is ready.
const GREETING_ROOM: usize = 64 * 1024;
/// How long the bridge has to say so: ssh's own time to connect, the
/// daemon's to start there, and room.
const READY_WITHIN: Duration = Duration::from_secs(25);
/// How much of what ssh says is kept, to say why it failed.
const STDERR_ROOM: u64 = 16 * 1024;

/// Whether ssh may ask the person anything now: where they are at a
/// terminal, and the UI does not hold it.
pub(crate) fn how_here() -> How {
    use std::io::IsTerminal as _;
    if std::io::stdin().is_terminal() && std::io::stderr().is_terminal() {
        How::Interactive
    } else {
        How::Background
    }
}

/// A connection to Muster on `remote`, through ssh.
pub(crate) async fn connect(remote: &Remote, how: How) -> Result<Connection, ClientError> {
    let bridge = format!("{REMOTE_PATH}; exec muster bridge");
    let control = control_dir();
    let mut child = tokio::process::Command::new("ssh")
        .args(ssh_args(how, false, &remote.target, control.as_deref()))
        .arg(format!("sh -c {}", quoted(&bridge)))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // Not killed with the connection: closing its input lets ssh carry
        // what is still on its way (a command's last keys) before it ends,
        // and the keepalives end it where the link is gone.
        .kill_on_drop(false)
        .spawn()
        .map_err(|error| ClientError::Carrier(format!("could not run ssh: {error}")))?;
    let (Some(writer), Some(mut reader)) = (child.stdin.take(), child.stdout.take()) else {
        return Err(ClientError::Carrier("ssh gave no way to talk".to_owned()));
    };
    let name = &remote.name;
    let Ok(Ok(after)) = tokio::time::timeout(READY_WITHIN, read_until_ready(&mut reader)).await
    else {
        return Err(failed(&mut child, name).await);
    };
    let reader = Cursor::new(after).chain(reader);
    match Connection::over(reader, writer, crate::commands::CLIENT).await {
        Ok(connection) => Ok(connection.holding(child)),
        Err(ClientError::Incompatible(theirs)) => Err(ClientError::Carrier(format!(
            "Muster on {name} is {}, another version than this one: `muster ssh {name} \
             --install` brings it to {}",
            theirs.daemon_version,
            env!("CARGO_PKG_VERSION")
        ))),
        Err(ClientError::Closed | ClientError::Transport(_)) => Err(failed(&mut child, name).await),
        Err(error) => Err(error),
    }
}

/// Why the link failed, from what ssh said as it ended, with what to do.
async fn failed(child: &mut tokio::process::Child, name: &str) -> ClientError {
    let _ = tokio::time::timeout(Duration::from_secs(2), child.wait()).await;
    let _ = child.start_kill();
    let mut said = String::new();
    if let Some(stderr) = child.stderr.take() {
        let mut kept = Vec::new();
        let _ = tokio::time::timeout(
            Duration::from_secs(1),
            stderr.take(STDERR_ROOM).read_to_end(&mut kept),
        )
        .await;
        said = String::from_utf8_lossy(&kept).into_owned();
    }
    if let Some(hint) = hint(&said, name) {
        return ClientError::Carrier(hint);
    }
    let last: Vec<&str> = said
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    let tail = last[last.len().saturating_sub(3)..].join(" / ");
    ClientError::Carrier(if tail.is_empty() {
        format!("could not reach Muster on {name} over ssh: `muster ssh {name} --check` says why")
    } else {
        format!(
            "could not reach Muster on {name} over ssh: {}",
            muster_core::text::one_line(&tail)
        )
    })
}

/// Reads past what a login printed, up to and with the bridge's ready
/// line; what came after it in the same read is the protocol's, given
/// back. Refused where the stream ends first, or a login prints more than
/// [`GREETING_ROOM`].
pub(crate) async fn read_until_ready(
    reader: &mut (impl AsyncRead + Unpin),
) -> std::io::Result<Vec<u8>> {
    let marker = format!("{BRIDGE_READY}\n");
    let marker = marker.as_bytes();
    let mut seen = Vec::new();
    let mut chunk = [0_u8; 4096];
    loop {
        if let Some(at) = seen
            .windows(marker.len())
            .position(|window| window == marker)
        {
            return Ok(seen.split_off(at + marker.len()));
        }
        if seen.len() > GREETING_ROOM {
            return Err(std::io::Error::other("no ready line from the bridge"));
        }
        let read = reader.read(&mut chunk).await?;
        if read == 0 {
            return Err(std::io::ErrorKind::UnexpectedEof.into());
        }
        seen.extend_from_slice(&chunk[..read]);
    }
}

/// [`connect`] to `remote` without asking anything, for the UI to call
/// each time its link is lost.
pub(crate) fn connector(remote: &'static Remote) -> muster_tui::Connector {
    Box::new(move || {
        let connecting: std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<Connection, ClientError>> + Send>,
        > = Box::pin(connect(remote, How::Background));
        connecting
    })
}

/// `muster bridge`, which `--on` has ssh run on the machine: joins its
/// standard input and output to this machine's daemon, starting it where
/// none runs, says it is ready, and copies both ways until either side
/// closes. The person's forwarded ssh agent is linked for the sessions.
pub(crate) async fn bridge(paths: &Paths) -> Result<ExitCode, Box<dyn std::error::Error>> {
    if let Some(forwarded) = std::env::var_os("SSH_AUTH_SOCK") {
        link_agent(&paths.runtime_dir, Path::new(&forwarded));
    }
    let socket = paths.socket();
    let stream = if let Ok(stream) = tokio::net::UnixStream::connect(&socket).await {
        stream
    } else {
        crate::daemon_start::start(paths)?;
        let deadline = crate::instant_now() + muster_tui::START_WAIT;
        loop {
            match tokio::net::UnixStream::connect(&socket).await {
                Ok(stream) => break stream,
                Err(_) if crate::instant_now() < deadline => {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
                Err(error) => return Err(format!("the daemon did not start: {error}").into()),
            }
        }
    };
    let (mut from_daemon, mut to_daemon) = stream.into_split();
    let mut stdout = tokio::io::stdout();
    stdout
        .write_all(format!("{BRIDGE_READY}\n").as_bytes())
        .await?;
    stdout.flush().await?;
    let to = async {
        let mut stdin = tokio::io::stdin();
        let mut buffer = vec![0_u8; 64 * 1024];
        loop {
            let read = stdin.read(&mut buffer).await?;
            if read == 0 {
                return Ok::<_, std::io::Error>(());
            }
            to_daemon.write_all(&buffer[..read]).await?;
        }
    };
    let from = async {
        let mut buffer = vec![0_u8; 64 * 1024];
        loop {
            let read = from_daemon.read(&mut buffer).await?;
            if read == 0 {
                return Ok::<_, std::io::Error>(());
            }
            stdout.write_all(&buffer[..read]).await?;
            stdout.flush().await?;
        }
    };
    tokio::select! {
        _ = to => {}
        _ = from => {}
    }
    // Ended at once: a read of standard input still waiting would hold the
    // process until ssh closed it.
    std::process::exit(0);
}

/// Points Muster's agent link in `runtime` at `forwarded`, the ssh agent
/// the person's ssh forwarded to this connection, where it is a socket of
/// theirs; anything else is left alone. Replaced in one step, so a session
/// never finds it missing.
pub(crate) fn link_agent(runtime: &Path, forwarded: &Path) -> bool {
    let uid = nix::unistd::getuid().as_raw();
    let theirs = std::fs::metadata(forwarded)
        .is_ok_and(|meta| meta.file_type().is_socket() && meta.uid() == uid);
    if !theirs || !runtime.is_dir() {
        return false;
    }
    let link = runtime.join(AGENT_LINK);
    let partial = runtime.join(format!("{AGENT_LINK}.{}", std::process::id()));
    let _ = std::fs::remove_file(&partial);
    std::os::unix::fs::symlink(forwarded, &partial).is_ok()
        && std::fs::rename(&partial, &link).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_login_s_greeting_is_read_past_and_the_protocol_kept_arc_27() {
        let (mut ours, mut theirs) = tokio::io::duplex(1024);
        let writing = async move {
            theirs
                .write_all(b"Welcome to Ubuntu\nconda: base\n")
                .await
                .unwrap();
            theirs
                .write_all(format!("{BRIDGE_READY}\nFRAME").as_bytes())
                .await
                .unwrap();
        };
        let (after, ()) = tokio::join!(read_until_ready(&mut ours), writing);
        assert_eq!(after.unwrap(), b"FRAME");
    }

    #[tokio::test]
    async fn no_ready_line_is_refused() {
        let mut ended: &[u8] = b"bash: muster: command not found\n";
        assert!(read_until_ready(&mut ended).await.is_err());
        let chatty = vec![b'x'; GREETING_ROOM + 10];
        let mut chatty: &[u8] = &chatty;
        assert!(read_until_ready(&mut chatty).await.is_err());
    }

    #[test]
    fn the_agent_link_follows_a_socket_of_the_person_s_and_nothing_else() {
        let runtime = tempfile::tempdir().unwrap();
        let agents = tempfile::tempdir().unwrap();
        let socket = agents.path().join("agent.1");
        let _listening = std::os::unix::net::UnixListener::bind(&socket).unwrap();
        assert!(link_agent(runtime.path(), &socket));
        let link = runtime.path().join(AGENT_LINK);
        assert_eq!(std::fs::read_link(&link).unwrap(), socket);
        let next = agents.path().join("agent.2");
        let _next = std::os::unix::net::UnixListener::bind(&next).unwrap();
        assert!(link_agent(runtime.path(), &next), "the next connection's");
        assert_eq!(std::fs::read_link(&link).unwrap(), next);
        let file = agents.path().join("not-a-socket");
        std::fs::write(&file, "").unwrap();
        assert!(!link_agent(runtime.path(), &file));
        assert_eq!(std::fs::read_link(&link).unwrap(), next, "left alone");
    }
}

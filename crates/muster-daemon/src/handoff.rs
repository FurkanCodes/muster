//! The daemon's restart in place (`muster reload`, `ARCHITECTURE.md` §4):
//! the daemon replaces its own program with the one installed now, as the
//! same process, so every session's program stays its child and runs on
//! untouched, and Muster carries on in the new version.
//!
//! The running daemon hands every terminal over (`pty::Handed`), writes what
//! goes with each (its ring, modes and size, its status machine, its hook
//! token) to a file that has no name, keeps that file, the terminals, its
//! two sockets and its lock open across `exec`, and execs the new program
//! with `--adopt <descriptor>`. The new program reads the file and takes it
//! all over before anything else; clients, cut off, connect again.
//!
//! Nothing that would lose a session is done before the new program has
//! said it can take over (`--can-adopt`), and if `exec` fails the daemon
//! takes its sessions back and carries on as it was.

pub(crate) mod fds;

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, Write};
use std::os::fd::{AsRawFd, OwnedFd, RawFd};
use std::os::unix::fs::{FileTypeExt, OpenOptionsExt};
use std::os::unix::net::UnixListener;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use muster_core::SessionId;
use serde::{Deserialize, Serialize};

/// The handoff's format: a daemon takes over only from one whose format
/// it reads, and says so when asked (`--can-adopt`).
pub(crate) const FORMAT: u32 = 1;

/// How long the new program may take to say whether it can take over.
const ASK_LIMIT: Duration = Duration::from_secs(10);

/// What a daemon hands its next version, written to the handoff's file.
#[derive(Debug, Serialize, Deserialize)]
struct Manifest {
    format: u32,
    /// The version handing over, for the log.
    from: String,
    /// The descriptors carried across `exec`, by number.
    lock: RawFd,
    socket: RawFd,
    hook_socket: RawFd,
    sessions: Vec<CarriedSession>,
}

/// A running session as carried: everything the Supervisor knows of it.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct CarriedSession {
    pub(crate) id: SessionId,
    pub(crate) harness: String,
    /// The token its hooks carry (`ARC-11`).
    pub(crate) token: Option<crate::secrets::Token>,
    /// How long it had been running.
    pub(crate) running_for: Duration,
    pub(crate) resumed: bool,
    pub(crate) machine: muster_state::session::Carried,
    /// Its terminal's descriptor across `exec`; set as the handoff is
    /// written.
    #[serde(default)]
    master: RawFd,
    pub(crate) terminal: crate::pty::Carried,
    /// The turn under way, by the id its harness's records know it by;
    /// none from a daemon before it was carried.
    #[serde(default)]
    pub(crate) turn: Option<String>,
    /// The coding agent running in front in it, where it is a terminal
    /// (`ARC-28`); none from a daemon before it was carried.
    #[serde(default)]
    pub(crate) agent: Option<String>,
}

impl CarriedSession {
    pub(crate) fn new(
        id: SessionId,
        harness: String,
        token: Option<crate::secrets::Token>,
        running_for: Duration,
        resumed: bool,
        machine: muster_state::session::Carried,
        terminal: crate::pty::Carried,
    ) -> Self {
        Self {
            id,
            harness,
            token,
            running_for,
            resumed,
            machine,
            master: -1,
            terminal,
            turn: None,
            agent: None,
        }
    }
}

/// A session handed over, with its terminal: carried to the next version,
/// or taken back if there is none.
#[derive(Debug)]
pub(crate) struct HandedSession {
    pub(crate) session: CarriedSession,
    pub(crate) master: OwnedFd,
}

/// What a daemon taking over inherits.
#[derive(Debug)]
pub(crate) struct Inherited {
    /// The version that handed over.
    pub(crate) from: String,
    pub(crate) lock: File,
    pub(crate) socket: UnixListener,
    pub(crate) hook_socket: UnixListener,
    pub(crate) sessions: Vec<HandedSession>,
}

/// The program installed now, which a restart in place runs. Linux names
/// a program replaced since it started `… (deleted)`; the new one is at
/// the path without it.
pub(crate) fn next_program() -> io::Result<PathBuf> {
    let program = std::env::current_exe()?;
    let text = program.to_string_lossy();
    Ok(match text.strip_suffix(" (deleted)") {
        Some(path) => PathBuf::from(path),
        None => program,
    })
}

/// Why the daemon could not restart in place; it carries on as it was.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ReloadError {
    #[error("cannot tell which program to run: {0}")]
    NoProgram(io::Error),
    #[error("{} cannot take over this daemon (an older Muster?)", .0.display())]
    Refused(PathBuf),
    #[error("cannot run {}: {error}", .program.display())]
    Unrunnable { program: PathBuf, error: io::Error },
    #[error("{} did not say whether it can take over", .0.display())]
    NoAnswer(PathBuf),
    #[error("could not start {}: {error}", .program.display())]
    NotStarted { program: PathBuf, error: io::Error },
}

/// Whether `program` can take over from this daemon: it must say so for
/// this handoff's format, as an older Muster cannot.
pub(crate) async fn can_take_over(program: &Path) -> Result<(), ReloadError> {
    let asked = tokio::process::Command::new(program)
        .args(["daemon", "--can-adopt", &FORMAT.to_string()])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .status();
    match tokio::time::timeout(ASK_LIMIT, asked).await {
        Ok(Ok(status)) if status.success() => Ok(()),
        Ok(Ok(_)) => Err(ReloadError::Refused(program.to_owned())),
        Ok(Err(error)) => Err(ReloadError::Unrunnable {
            program: program.to_owned(),
            error,
        }),
        Err(_) => Err(ReloadError::NoAnswer(program.to_owned())),
    }
}

/// Replaces this process with `program`, handing it `sessions` and the
/// lock and sockets. Returns only if that failed, with why and the
/// sessions, every descriptor closed on `exec` again.
#[expect(
    clippy::too_many_arguments,
    reason = "the exec boundary names everything it hands across, each a different thing"
)]
pub(crate) fn exec_into(
    program: &Path,
    arguments: &[String],
    private_dir: &Path,
    from: &str,
    lock: &File,
    socket: &UnixListener,
    hook_socket: &UnixListener,
    sessions: Vec<HandedSession>,
) -> (io::Error, Vec<HandedSession>) {
    let mut manifest = Manifest {
        format: FORMAT,
        from: from.to_owned(),
        lock: lock.as_raw_fd(),
        socket: socket.as_raw_fd(),
        hook_socket: hook_socket.as_raw_fd(),
        sessions: Vec::new(),
    };
    let (mut carried, masters): (Vec<CarriedSession>, Vec<OwnedFd>) = sessions
        .into_iter()
        .map(|handed| (handed.session, handed.master))
        .unzip();
    for (session, master) in carried.iter_mut().zip(&masters) {
        session.master = master.as_raw_fd();
    }
    manifest.sessions = carried;
    let back = |manifest: Manifest, masters: Vec<OwnedFd>| -> Vec<HandedSession> {
        let _ = fds::close_on_exec(lock);
        let _ = fds::close_on_exec(socket);
        let _ = fds::close_on_exec(hook_socket);
        manifest
            .sessions
            .into_iter()
            .zip(masters)
            .map(|(session, master)| {
                let _ = fds::close_on_exec(&master);
                HandedSession { session, master }
            })
            .collect()
    };
    let state = match write_state(private_dir, &manifest) {
        Ok(state) => state,
        Err(error) => return (error, back(manifest, masters)),
    };
    let kept = fds::keep_across_exec(lock)
        .and_then(|()| fds::keep_across_exec(socket))
        .and_then(|()| fds::keep_across_exec(hook_socket))
        .and_then(|()| fds::keep_across_exec(&state))
        .and_then(|()| masters.iter().try_for_each(fds::keep_across_exec));
    if let Err(error) = kept {
        return (error, back(manifest, masters));
    }
    let error = std::process::Command::new(program)
        .args(arguments)
        .arg("--adopt")
        .arg(state.as_raw_fd().to_string())
        .exec();
    (error, back(manifest, masters))
}

/// Writes the handoff to a file only this user can read, its name removed
/// at once: the descriptor alone carries it, and nothing is left behind.
fn write_state(dir: &Path, manifest: &Manifest) -> io::Result<File> {
    let path = dir.join(format!(".handoff-{}", std::process::id()));
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)?;
    std::fs::remove_file(&path)?;
    let bytes = rmp_serde::to_vec_named(manifest).map_err(io::Error::other)?;
    file.write_all(&bytes)?;
    file.flush()?;
    file.rewind()?;
    Ok(file)
}

/// Takes over what the daemon's previous version handed over in the file
/// at descriptor `state`.
pub(crate) fn inherit(state: RawFd) -> io::Result<Inherited> {
    let mut file = File::from(fds::inherit(state)?);
    if !file.metadata()?.is_file() {
        return Err(io::Error::other("the handoff is not a file"));
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    let manifest: Manifest = rmp_serde::from_slice(&bytes).map_err(io::Error::other)?;
    if manifest.format != FORMAT {
        return Err(io::Error::other(format!(
            "the handoff is in format {}, not {FORMAT}",
            manifest.format
        )));
    }
    let lock = File::from(fds::inherit(manifest.lock)?);
    let socket = listener(manifest.socket)?;
    let hook_socket = listener(manifest.hook_socket)?;
    let mut sessions = Vec::new();
    for session in manifest.sessions {
        let master = File::from(fds::inherit(session.master)?);
        if !master.metadata()?.file_type().is_char_device() {
            return Err(io::Error::other("a handed terminal is not a terminal"));
        }
        sessions.push(HandedSession {
            session,
            master: OwnedFd::from(master),
        });
    }
    Ok(Inherited {
        from: manifest.from,
        lock,
        socket,
        hook_socket,
        sessions,
    })
}

fn listener(fd: RawFd) -> io::Result<UnixListener> {
    let file = File::from(fds::inherit(fd)?);
    if !file.metadata()?.file_type().is_socket() {
        return Err(io::Error::other("a handed socket is not a socket"));
    }
    Ok(UnixListener::from(OwnedFd::from(file)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn the_handoff_is_written_with_no_name_and_read_back() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = Manifest {
            format: FORMAT,
            from: "0.2.0".to_owned(),
            lock: 7,
            socket: 8,
            hook_socket: 9,
            sessions: Vec::new(),
        };
        let mut file = write_state(dir.path(), &manifest).unwrap();
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).unwrap();
        let read: Manifest = rmp_serde::from_slice(&bytes).unwrap();
        assert_eq!(
            (read.format, read.from, read.lock),
            (FORMAT, "0.2.0".into(), 7)
        );
        assert_eq!(file.metadata().unwrap().permissions().mode() & 0o777, 0o600);
    }

    #[test]
    fn a_replaced_program_on_linux_is_found_where_it_was() {
        // What `current_exe` gives once the file has been replaced.
        let text = "/home/me/.local/bin/muster (deleted)";
        assert_eq!(
            text.strip_suffix(" (deleted)"),
            Some("/home/me/.local/bin/muster")
        );
        assert!(next_program().unwrap().is_absolute());
    }
}

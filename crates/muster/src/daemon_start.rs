//! Starting the daemon where none answers, as the UI does: for a command
//! that sets agents working, which the person should not have to start a
//! daemon for first.

use std::time::Duration;

use muster_client::{ClientError, Connection};
use muster_core::Paths;

/// How often a command looks for a daemon it started.
const LOOK_EVERY: Duration = Duration::from_millis(50);

/// A connection to the daemon, started first if none answers.
pub(crate) async fn connect(paths: &Paths) -> Result<Connection, Box<dyn std::error::Error>> {
    // There, the bridge starts the machine's daemon where none runs.
    if let Some(remote) = crate::ssh::remote() {
        return Ok(crate::ssh::connect(remote).await?);
    }
    let socket = paths.socket();
    match crate::commands::connect(&socket).await {
        Err(ClientError::Unreachable { .. }) => {}
        connected => return Ok(connected?),
    }
    start(paths)?;
    let deadline = crate::instant_now() + muster_tui::START_WAIT;
    loop {
        match crate::commands::connect(&socket).await {
            Err(ClientError::Unreachable { .. }) if crate::instant_now() < deadline => {
                tokio::time::sleep(LOOK_EVERY).await;
            }
            Err(ClientError::Unreachable { .. }) => {
                return Err(format!(
                    "the daemon did not start; its log is {}",
                    paths.runtime_dir.join("daemon.log").display()
                )
                .into());
            }
            connected => return Ok(connected?),
        }
    }
}

/// Starts `muster daemon --detach`, logging to `daemon.log` in the runtime
/// directory. That starts the daemon itself apart from this process and
/// returns at once, so the daemon is never the UI's child: whatever ends
/// the UI, its whole process tree included, leaves the daemon and its
/// sessions running (`PRD-09`). A thread waits for the short-lived starter
/// so it never lingers as a zombie.
pub(crate) fn start(paths: &Paths) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&paths.runtime_dir)?;
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(paths.runtime_dir.join("daemon.log"))?;
    let mut daemon = std::process::Command::new(std::env::current_exe()?)
        .args(["daemon", "--detach"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(log)
        .spawn()?;
    std::thread::Builder::new()
        .name("muster-reap-daemon".to_owned())
        .spawn(move || {
            let _ = daemon.wait();
        })?;
    Ok(())
}

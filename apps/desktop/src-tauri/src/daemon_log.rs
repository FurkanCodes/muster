//! The daemon's log, shown when the person asks from the banner that says
//! the daemon is out of reach (*Show its log*): `daemon.log` in Muster's
//! runtime folder, where the app and the terminal UI write the daemon's
//! errors as they start it. macOS opens it in the app that opens logs; the
//! webview never reads it.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use tauri::State;

use crate::contract::BridgeError;

/// Where the daemon's log is, for this app's daemon.
#[derive(Debug)]
pub(crate) struct DaemonLog(pub(crate) PathBuf);

fn invalid(message: impl Into<String>) -> BridgeError {
    BridgeError::Invalid {
        message: message.into(),
    }
}

/// `open` showing `file`: the file is one argument, never shell code.
fn opener(file: &Path) -> tokio::process::Command {
    let mut command = tokio::process::Command::new("/usr/bin/open");
    command
        .arg(file)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    command
}

/// Opens the daemon's log; says why not where there is none yet.
#[tauri::command]
pub(crate) async fn show_daemon_log(log: State<'_, DaemonLog>) -> Result<(), BridgeError> {
    let file = log.0.clone();
    if !file.is_file() {
        return Err(invalid(format!(
            "There is no daemon.log yet in {}.",
            file.parent().unwrap_or(&file).display()
        )));
    }
    if crate::e2e::recorded("revealed", &file.to_string_lossy()) {
        return Ok(());
    }
    if !cfg!(target_os = "macos") {
        return Err(invalid(format!("The log is {}.", file.display())));
    }
    let status = tokio::time::timeout(Duration::from_secs(5), opener(&file).status())
        .await
        .map_err(|_| invalid("The log did not open in time. Try again."))?
        .map_err(|error| invalid(format!("The log could not be opened: {error}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(invalid(format!(
            "The log could not be opened. It is {}.",
            file.display()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_log_is_one_argument_to_open_never_shell_code() {
        let command = opener(Path::new("/tmp/mu $(touch x)/daemon.log"));
        let command = command.as_std();
        assert_eq!(command.get_program(), "/usr/bin/open");
        let args: Vec<_> = command.get_args().collect();
        assert_eq!(args, ["/tmp/mu $(touch x)/daemon.log"]);
    }
}

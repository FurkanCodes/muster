//! What Settings asks of `muster` and of the Mac, each only on the person's
//! word (`ARC-24`): the daemon restarted in place onto the Muster installed
//! now, as `muster reload` does it, by running it; the Mac's notification
//! settings opened at Muster's helper, which sends the notifications
//! (`muster::mac_app`); and Muster's own `settings.toml` opened in the
//! person's editor. Nothing here takes a path or a word from the webview.
//! A build for the end-to-end tests records the two that reach outside the
//! app instead (`e2e.rs`); the restart reaches only the daemon the app is
//! given, so it runs in those builds too.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use muster_core::Paths;

use crate::contract::BridgeError;

/// Muster's helper on a Mac, which sends its notifications (`muster::mac_app::BUNDLE_ID`).
const HELPER: &str = "dev.muster.Muster";

/// The Notifications pane of System Settings, at Muster's helper.
fn notifications_pane() -> String {
    format!("x-apple.systempreferences:com.apple.Notifications-Settings.extension?id={HELPER}")
}

/// How long `muster reload` may take: it waits up to 30 seconds for the
/// daemon to answer again.
const RELOAD_AT_MOST: Duration = Duration::from_secs(45);

fn invalid(message: impl Into<String>) -> BridgeError {
    BridgeError::Invalid {
        message: message.into(),
    }
}

/// What `muster` printed, its last line that says something; a failure's
/// `muster:` taken off, as the window says it.
fn said(text: &[u8]) -> String {
    let text = String::from_utf8_lossy(text);
    let line = text
        .lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty())
        .unwrap_or_default();
    line.strip_prefix("muster: ")
        .or_else(|| line.strip_prefix("error: "))
        .unwrap_or(line)
        .to_owned()
}

/// `muster reload`, as the terminal would run it.
fn reload_command(muster: &Path) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(muster);
    command
        .arg("reload")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    command
}

/// Moves the daemon onto the Muster installed now, every session running
/// on, as `muster reload` does: it is run, and what it says is the answer.
#[tauri::command]
pub(crate) async fn restart_in_place() -> Result<String, BridgeError> {
    let muster = crate::link::installed_muster().ok_or_else(|| {
        invalid("Muster is not installed: no `muster` to restart the daemon with.")
    })?;
    let output = tokio::time::timeout(RELOAD_AT_MOST, reload_command(&muster).output())
        .await
        .map_err(|_| invalid("The daemon did not answer in time; `muster reload` gave up."))?
        .map_err(|error| invalid(format!("`muster reload` could not run: {error}")))?;
    if output.status.success() {
        Ok(said(&output.stdout))
    } else {
        Err(invalid(said(&output.stderr)))
    }
}

/// `open` with `args`, as the Mac opens what it is given; within a moment.
async fn open(args: &[&str], what: &str) -> Result<(), BridgeError> {
    let status = tokio::time::timeout(
        Duration::from_secs(5),
        tokio::process::Command::new("/usr/bin/open")
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .status(),
    )
    .await
    .map_err(|_| invalid(format!("{what} did not open in time. Try again.")))?
    .map_err(|error| invalid(format!("{what} could not be opened: {error}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(invalid(format!("{what} could not be opened. Try again.")))
    }
}

/// Opens System Settings at the notifications of Muster's helper.
#[tauri::command]
pub(crate) async fn open_notification_settings() -> Result<(), BridgeError> {
    let pane = notifications_pane();
    if crate::e2e::recorded("opened", &pane) {
        return Ok(());
    }
    open(&[&pane], "System Settings").await
}

/// Muster's own settings, where the daemon reads them.
fn settings_file() -> Result<PathBuf, BridgeError> {
    let paths = Paths::resolve(|key| std::env::var_os(key), nix::unistd::getuid().as_raw())
        .map_err(|error| invalid(format!("Muster's settings could not be found: {error}")))?;
    Ok(paths.config_dir.join("settings.toml"))
}

/// Opens Muster's own `settings.toml` in the person's editor (`open -t`),
/// made empty first where Muster has written none yet.
#[tauri::command]
pub(crate) async fn edit_settings_file() -> Result<(), BridgeError> {
    let file = settings_file()?;
    if !file.exists() {
        if let Some(folder) = file.parent() {
            std::fs::create_dir_all(folder).map_err(|error| {
                invalid(format!("{} could not be made: {error}", folder.display()))
            })?;
        }
        std::fs::write(&file, "")
            .map_err(|error| invalid(format!("{} could not be made: {error}", file.display())))?;
    }
    let shown = file.to_string_lossy();
    if crate::e2e::recorded("edited", &shown) {
        return Ok(());
    }
    open(&["-t", "--", &shown], "settings.toml").await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arc_24_the_notifications_opened_are_muster_s_helper_s() {
        assert_eq!(
            notifications_pane(),
            "x-apple.systempreferences:com.apple.Notifications-Settings.extension?id=dev.muster.Muster"
        );
    }

    #[test]
    fn arc_24_what_muster_reload_says_is_its_last_line_without_its_name() {
        assert_eq!(
            said(b"The daemon is now muster 0.4.1, its sessions running on.\n"),
            "The daemon is now muster 0.4.1, its sessions running on."
        );
        assert_eq!(
            said(b"muster: the daemon (pid 7) has not answered for 30 seconds; see /x/daemon.log\n\n"),
            "the daemon (pid 7) has not answered for 30 seconds; see /x/daemon.log"
        );
        assert_eq!(said(b""), "");
    }

    #[test]
    fn arc_24_the_restart_runs_muster_reload_and_nothing_else() {
        let command = reload_command(Path::new("/opt/muster/bin/muster"));
        let command = command.as_std();
        assert_eq!(command.get_program(), "/opt/muster/bin/muster");
        assert_eq!(command.get_args().collect::<Vec<_>>(), ["reload"]);
    }
}

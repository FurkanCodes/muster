//! Hearing a sound as the person chooses it for a notification (`PRODUCT.md`
//! §6: "heard as they choose it"), played as the terminal UI plays it
//! (`muster_tui::runtime::hear`): the system's own `afplay`, on a Mac only,
//! and only a name with no folder in it, one of the Mac's own sounds or a
//! file in the person's `~/Library/Sounds`. The webview never waits for it.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use muster_core::telling::{MAC_SOUNDS, Sound};

use crate::contract::BridgeError;

/// The longest a sound plays before it is stopped: a notification's lasts a
/// moment (`ARC-13`, every wait has a deadline).
const PLAYS_AT_MOST: Duration = Duration::from_secs(30);

/// The file `sound` names, or `None` for one that is no file of its own
/// (the Mac's alert, none, or the person's with no home to find it in).
fn sound_file(sound: &str, home: Option<&Path>) -> Result<Option<PathBuf>, BridgeError> {
    let sound = Sound::parse(sound).map_err(|message| BridgeError::Invalid { message })?;
    let Sound::Named(name) = sound else {
        return Ok(None);
    };
    if name.contains(['/', '\\']) || name.starts_with('.') {
        return Err(BridgeError::Invalid {
            message: format!("{name:?} is a sound's name with a folder in it"),
        });
    }
    if MAC_SOUNDS.contains(&name.as_str()) {
        return Ok(Some(PathBuf::from(format!(
            "/System/Library/Sounds/{name}.aiff"
        ))));
    }
    Ok(home.map(|home| home.join("Library/Sounds").join(name)))
}

/// `afplay` playing `file`: the file is one argument, never shell code.
fn player(file: &Path) -> tokio::process::Command {
    let mut command = tokio::process::Command::new("/usr/bin/afplay");
    command
        .arg(file)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}

/// Plays `sound`, as Settings › Notifications chooses it; returns once it
/// has started, and plays nothing where it cannot (not on a Mac, the Mac's
/// alert, none).
#[tauri::command]
pub(crate) async fn hear(sound: String) -> Result<(), BridgeError> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let Some(file) = sound_file(&sound, home.as_deref())? else {
        return Ok(());
    };
    if crate::e2e::recorded("heard", &file.to_string_lossy()) || !cfg!(target_os = "macos") {
        return Ok(());
    }
    let mut child = player(&file)
        .spawn()
        .map_err(|error| BridgeError::Invalid {
            message: format!("The sound could not be played: {error}"),
        })?;
    // Reaped as it ends, or stopped at its deadline: it plays for a moment.
    tauri::async_runtime::spawn(async move {
        if tokio::time::timeout(PLAYS_AT_MOST, child.wait())
            .await
            .is_err()
        {
            let _ = child.kill().await;
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOME: &str = "/Users/p";

    fn file(sound: &str) -> Result<Option<PathBuf>, BridgeError> {
        sound_file(sound, Some(Path::new(HOME)))
    }

    #[test]
    fn a_mac_sound_plays_from_the_system_and_the_person_s_from_their_own_sounds() {
        assert_eq!(
            file("Glass").ok().flatten(),
            Some(PathBuf::from("/System/Library/Sounds/Glass.aiff"))
        );
        assert_eq!(
            file("notification.wav").ok().flatten(),
            Some(PathBuf::from("/Users/p/Library/Sounds/notification.wav"))
        );
        assert_eq!(sound_file("notification.wav", None).ok().flatten(), None);
    }

    #[test]
    fn the_mac_s_alert_and_none_are_no_file_to_play() {
        assert_eq!(file("default").ok().flatten(), None);
        assert_eq!(file("none").ok().flatten(), None);
    }

    #[test]
    fn a_name_with_a_folder_in_it_is_refused() {
        for name in [
            "../../etc/passwd",
            "Sounds/x.wav",
            "a\\b.wav",
            ".hidden",
            "/System/Library/Sounds/Glass.aiff",
            "",
        ] {
            assert!(file(name).is_err(), "{name:?}");
        }
    }

    #[test]
    fn a_sound_is_one_argument_to_afplay_never_shell_code() {
        let command = player(Path::new("/Users/p/Library/Sounds/$(touch x).wav"));
        let command = command.as_std();
        assert_eq!(command.get_program(), "/usr/bin/afplay");
        let args: Vec<_> = command.get_args().collect();
        assert_eq!(args, ["/Users/p/Library/Sounds/$(touch x).wav"]);
    }
}

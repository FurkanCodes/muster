//! The window's own state that outlives it (which lines of *All sessions*
//! are folded, the theme): never the daemon's, which is never told (`ARC-24`).
//! The webview's storage writes to disk when it likes, and an app quit with
//! ⌘Q, or one that crashes, loses what it had not written; so the webview
//! hands each change here (`src/state/kept.ts`), and it is in a file before
//! the call returns. The file is `window.json` in the app's data folder,
//! written whole to a file beside it and renamed over it, so it is never
//! found half written.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tauri::State;

use crate::contract::BridgeError;

/// What is kept, by the webview's names for it, each a string it reads back.
type Values = BTreeMap<String, String>;

/// The kept state and its file.
#[derive(Debug)]
pub(crate) struct Kept {
    file: PathBuf,
    values: Mutex<Values>,
}

impl Kept {
    /// What `file` holds; nothing where it is missing or does not read.
    pub(crate) fn open(file: PathBuf) -> Self {
        let values = fs::read_to_string(&file)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        Self {
            file,
            values: Mutex::new(values),
        }
    }

    fn values(&self) -> std::sync::MutexGuard<'_, Values> {
        // A panic while writing leaves the values as they were before it.
        self.values
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Everything kept.
    pub(crate) fn all(&self) -> Values {
        self.values().clone()
    }

    /// Keeps `value` under `key`, or forgets `key` where it is `None`, and
    /// writes the file before it returns.
    pub(crate) fn keep(&self, key: String, value: Option<String>) -> std::io::Result<()> {
        let mut values = self.values();
        match value {
            Some(value) => values.insert(key, value),
            None => values.remove(&key),
        };
        write(&self.file, &values)
    }
}

fn write(file: &Path, values: &Values) -> std::io::Result<()> {
    if let Some(folder) = file.parent() {
        fs::create_dir_all(folder)?;
    }
    let text = serde_json::to_string_pretty(values).map_err(std::io::Error::other)?;
    let part = file.with_extension("json.part");
    fs::write(&part, text)?;
    fs::rename(&part, file)
}

/// Everything kept, read once as the window loads.
#[tauri::command]
pub(crate) fn kept(kept: State<'_, Kept>) -> Values {
    kept.all()
}

/// Keeps `value` under `key` (forgets it where `value` is null), in the file
/// before it returns: off the main thread, which a write must not hold up.
#[tauri::command(async)]
pub(crate) fn keep(
    kept: State<'_, Kept>,
    key: String,
    value: Option<String>,
) -> Result<(), BridgeError> {
    kept.keep(key, value).map_err(|error| BridgeError::Invalid {
        message: format!("could not keep the window's state: {error}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(folder: &tempfile::TempDir) -> PathBuf {
        folder.path().join("data").join("window.json")
    }

    #[test]
    fn arc_24_what_is_kept_is_in_the_file_as_keep_returns() {
        let folder = tempfile::tempdir().unwrap();
        let kept = Kept::open(file(&folder));
        assert_eq!(kept.all(), Values::new());
        kept.keep("muster.theme".into(), Some("light".into()))
            .unwrap();
        kept.keep("muster.sessions.folds".into(), Some("{}".into()))
            .unwrap();
        // A crash now loses nothing: another app reads it all.
        let again = Kept::open(file(&folder));
        assert_eq!(
            again.all(),
            Values::from([
                ("muster.sessions.folds".into(), "{}".into()),
                ("muster.theme".into(), "light".into()),
            ])
        );
        assert!(!file(&folder).with_extension("json.part").exists());
    }

    #[test]
    fn arc_24_a_key_kept_as_nothing_is_forgotten() {
        let folder = tempfile::tempdir().unwrap();
        let kept = Kept::open(file(&folder));
        kept.keep("muster.theme".into(), Some("light".into()))
            .unwrap();
        kept.keep("muster.theme".into(), None).unwrap();
        assert_eq!(Kept::open(file(&folder)).all(), Values::new());
    }

    #[test]
    fn arc_24_a_file_that_does_not_read_keeps_nothing_and_is_written_afresh() {
        let folder = tempfile::tempdir().unwrap();
        fs::create_dir_all(file(&folder).parent().unwrap()).unwrap();
        fs::write(file(&folder), "not json").unwrap();
        let kept = Kept::open(file(&folder));
        assert_eq!(kept.all(), Values::new());
        kept.keep("muster.theme".into(), Some("dark".into()))
            .unwrap();
        assert_eq!(
            Kept::open(file(&folder)).all(),
            Values::from([("muster.theme".into(), "dark".into())])
        );
    }
}

//! Where the daemon keeps its socket and its data.
//!
//! Resolution is a pure function of the environment so tests can point every
//! daemon at its own directories (`MUSTER_RUNTIME_DIR`, `MUSTER_DATA_DIR`).

use std::ffi::OsString;
use std::path::PathBuf;

/// Unix socket paths longer than this fail to bind on macOS (the limit is 104
/// bytes including the terminator); checked up front for a clear message.
pub const MAX_SOCKET_PATH_LEN: usize = 100;

/// The daemon's directories.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Paths {
    /// Short-lived state: the socket and the lock file. Mode 0700.
    pub runtime_dir: PathBuf,
    /// Lasting state: the database.
    pub data_dir: PathBuf,
}

/// Why the directories could not be worked out.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PathsError {
    #[error("HOME is not set, so there is nowhere to keep Muster's data; set MUSTER_DATA_DIR")]
    NoHome,
    #[error(
        "the socket path {0:?} is too long for a Unix socket; set MUSTER_RUNTIME_DIR to a shorter directory"
    )]
    SocketPathTooLong(PathBuf),
}

impl Paths {
    /// Works out the directories from environment variables, read through
    /// `env`. `uid` names the fallback runtime directory.
    ///
    /// - runtime: `MUSTER_RUNTIME_DIR`, else `$XDG_RUNTIME_DIR/muster`, else
    ///   `${TMPDIR:-/tmp}/muster-<uid>`;
    /// - data: `MUSTER_DATA_DIR`, else the platform's data directory.
    pub fn resolve(env: impl Fn(&str) -> Option<OsString>, uid: u32) -> Result<Self, PathsError> {
        let var = |key: &str| {
            env(key)
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        };

        let runtime_dir = var("MUSTER_RUNTIME_DIR")
            .or_else(|| var("XDG_RUNTIME_DIR").map(|dir| dir.join("muster")))
            .unwrap_or_else(|| {
                var("TMPDIR")
                    .unwrap_or_else(|| PathBuf::from("/tmp"))
                    .join(format!("muster-{uid}"))
            });

        let data_dir = match var("MUSTER_DATA_DIR") {
            Some(dir) => dir,
            None => default_data_dir(&var)?,
        };

        let paths = Self {
            runtime_dir,
            data_dir,
        };
        let socket = paths.socket();
        if socket.as_os_str().len() > MAX_SOCKET_PATH_LEN {
            return Err(PathsError::SocketPathTooLong(socket));
        }
        Ok(paths)
    }

    /// The daemon's Unix socket.
    pub fn socket(&self) -> PathBuf {
        self.runtime_dir.join("daemon.sock")
    }

    /// Held by the running daemon so a second one refuses to start.
    pub fn lock_file(&self) -> PathBuf {
        self.runtime_dir.join("daemon.lock")
    }

    /// The SQLite database.
    pub fn database(&self) -> PathBuf {
        self.data_dir.join("muster.db")
    }
}

fn default_data_dir(var: &impl Fn(&str) -> Option<PathBuf>) -> Result<PathBuf, PathsError> {
    if cfg!(target_os = "macos") {
        let home = var("HOME").ok_or(PathsError::NoHome)?;
        return Ok(home.join("Library/Application Support/muster"));
    }
    if let Some(data_home) = var("XDG_DATA_HOME") {
        return Ok(data_home.join("muster"));
    }
    let home = var("HOME").ok_or(PathsError::NoHome)?;
    Ok(home.join(".local/share/muster"))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn resolve(vars: &[(&str, &str)]) -> Result<Paths, PathsError> {
        let vars: HashMap<String, OsString> = vars
            .iter()
            .map(|(key, value)| ((*key).to_owned(), OsString::from(value)))
            .collect();
        Paths::resolve(|key| vars.get(key).cloned(), 501)
    }

    #[test]
    fn explicit_directories_win() {
        let paths = resolve(&[
            ("MUSTER_RUNTIME_DIR", "/r"),
            ("MUSTER_DATA_DIR", "/d"),
            ("XDG_RUNTIME_DIR", "/x"),
            ("HOME", "/home/u"),
        ])
        .unwrap();
        assert_eq!(paths.socket(), PathBuf::from("/r/daemon.sock"));
        assert_eq!(paths.lock_file(), PathBuf::from("/r/daemon.lock"));
        assert_eq!(paths.database(), PathBuf::from("/d/muster.db"));
    }

    #[test]
    fn runtime_falls_back_to_xdg_then_tmpdir_then_tmp() {
        let xdg = resolve(&[("XDG_RUNTIME_DIR", "/run/user/501"), ("HOME", "/h")]).unwrap();
        assert_eq!(xdg.runtime_dir, PathBuf::from("/run/user/501/muster"));

        let tmpdir = resolve(&[("TMPDIR", "/var/t"), ("HOME", "/h")]).unwrap();
        assert_eq!(tmpdir.runtime_dir, PathBuf::from("/var/t/muster-501"));

        let bare = resolve(&[("HOME", "/h")]).unwrap();
        assert_eq!(bare.runtime_dir, PathBuf::from("/tmp/muster-501"));
    }

    #[test]
    fn empty_variables_count_as_unset() {
        let paths = resolve(&[("MUSTER_RUNTIME_DIR", ""), ("HOME", "/h")]).unwrap();
        assert_eq!(paths.runtime_dir, PathBuf::from("/tmp/muster-501"));
    }

    #[test]
    fn data_dir_needs_home_unless_set() {
        assert_eq!(resolve(&[]), Err(PathsError::NoHome));
        assert!(resolve(&[("MUSTER_DATA_DIR", "/d")]).is_ok());
    }

    #[test]
    fn a_socket_path_too_long_to_bind_is_rejected() {
        let long = format!("/{}", "x".repeat(MAX_SOCKET_PATH_LEN));
        let error = resolve(&[("MUSTER_RUNTIME_DIR", &long), ("HOME", "/h")]).unwrap_err();
        assert!(matches!(error, PathsError::SocketPathTooLong(_)));
    }
}

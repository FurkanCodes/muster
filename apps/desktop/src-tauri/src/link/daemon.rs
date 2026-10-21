//! Finding `muster` and starting its daemon, as the terminal UI does
//! (`ARCHITECTURE.md` §4 *Lifecycle*): `muster daemon --detach`, which
//! starts the daemon apart from the app, so closing the app, or anything
//! that ends it, leaves the daemon and its sessions running (`PRD-09`).
//!
//! An app opened from the Finder has a bare `PATH` (`/usr/bin:/bin:…`):
//! `muster` is looked for where it is installed too, and the daemon is
//! given the person's own `PATH`, as their login shell has it, so their
//! agents are found as they are from a terminal.

use std::ffi::{OsStr, OsString};
use std::io::{self, Read};
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use muster_core::Paths;

use super::{Start, monotonic};

/// Where `muster` is: `MUSTER_BIN`, then where `install.sh` puts it
/// (`~/.local/bin`), then where `cargo install` does (`~/.cargo/bin`), then
/// each folder on `PATH`. `env` reads a variable, `is_program` says whether
/// a path is a program that can be run.
pub fn find_muster(
    env: impl Fn(&str) -> Option<OsString>,
    is_program: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    let named = env("MUSTER_BIN")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);
    let home = env("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);
    let installed = home
        .iter()
        .flat_map(|home| [home.join(".local/bin"), home.join(".cargo/bin")])
        .map(|folder| folder.join("muster"));
    let on_path = env("PATH")
        .map(|path| {
            std::env::split_paths(&path)
                .filter(|folder| folder.is_absolute())
                .map(|folder| folder.join("muster"))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    named
        .into_iter()
        .chain(installed)
        .chain(on_path)
        .find(|candidate| is_program(candidate))
}

/// `muster`, where [`find_muster`] finds it from the app's own variables.
pub fn installed_muster() -> Option<PathBuf> {
    find_muster(|key| std::env::var_os(key), is_program)
}

/// Whether `path` is a file anyone may run.
fn is_program(path: &Path) -> bool {
    path.metadata()
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

/// Starts the daemon for `paths` with `muster daemon --detach`, its errors
/// appended to `daemon.log` in the runtime folder as the terminal UI's are.
/// `muster` is looked for each time, so one installed while the app waits
/// is found. Blocking: the link calls it off its own task.
pub fn starter(paths: Paths) -> Start {
    Box::new(move || {
        let muster = find_muster(|key| std::env::var_os(key), is_program).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "Muster is not installed: no `muster` in MUSTER_BIN, ~/.local/bin, \
                 ~/.cargo/bin or on PATH",
            )
        })?;
        start(&muster, &paths)
    })
}

fn start(muster: &Path, paths: &Paths) -> io::Result<()> {
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&paths.runtime_dir)?;
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(paths.runtime_dir.join("daemon.log"))?;
    let shell = std::env::var_os("SHELL")
        .filter(|shell| !shell.is_empty())
        .unwrap_or_else(|| OsString::from("/bin/zsh"));
    let path = daemon_path(
        std::env::var_os("PATH").as_deref(),
        login_path(Path::new(&shell)).as_deref(),
        muster.parent(),
    );
    let mut daemon = Command::new(muster)
        .args(["daemon", "--detach"])
        .env("PATH", path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(log)
        .spawn()?;
    // `--detach` returns as soon as the daemon is started apart from it; a
    // thread waits for it, so it never lingers as a zombie.
    std::thread::Builder::new()
        .name("muster-reap-daemon".to_owned())
        .spawn(move || {
            let _ = daemon.wait();
        })?;
    Ok(())
}

/// The `PATH` the daemon is given: the app's own first, then the folders
/// the person's login shell adds, then `muster`'s own folder.
fn daemon_path(own: Option<&OsStr>, login: Option<&OsStr>, muster: Option<&Path>) -> OsString {
    let mut folders: Vec<PathBuf> = Vec::new();
    let lists = [own, login]
        .into_iter()
        .flatten()
        .flat_map(std::env::split_paths)
        .chain(muster.map(Path::to_path_buf));
    for folder in lists {
        if !folder.as_os_str().is_empty() && !folders.contains(&folder) {
            folders.push(folder);
        }
    }
    std::env::join_paths(folders).unwrap_or_default()
}

/// What the person's login shell sets `PATH` to, read as their terminal's
/// shell would (`-i -l`), between markers so whatever their files print is
/// passed over; `None` if it says nothing within [`LOGIN_WAIT`].
fn login_path(shell: &Path) -> Option<OsString> {
    let mut child = Command::new(shell)
        .args([
            "-i",
            "-l",
            "-c",
            "printf '\\n%s%s%s\\n' \"$1\" \"$PATH\" \"$1\"",
            "-",
        ])
        .arg(MARKER)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = monotonic() + LOGIN_WAIT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if monotonic() < deadline => std::thread::sleep(Duration::from_millis(10)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    let mut said = Vec::new();
    child.stdout.take()?.read_to_end(&mut said).ok()?;
    between_markers(&said).map(OsString::from)
}

/// How long the login shell may take to say its `PATH`.
const LOGIN_WAIT: Duration = Duration::from_secs(3);

/// Marks where the login shell's `PATH` starts and ends in what it prints.
const MARKER: &str = "__MUSTER_PATH__";

fn between_markers(said: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(said);
    let start = text.find(MARKER)? + MARKER.len();
    let end = start + text[start..].find(MARKER)?;
    let path = text[start..end].trim();
    (!path.is_empty()).then(|| path.to_owned())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn env(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
        let vars: HashMap<String, OsString> = vars
            .iter()
            .map(|(key, value)| ((*key).to_owned(), OsString::from(value)))
            .collect();
        move |key| vars.get(key).cloned()
    }

    #[test]
    fn muster_is_looked_for_where_it_is_installed_before_a_bare_path() {
        let programs = ["/home/p/.cargo/bin/muster", "/usr/bin/muster"];
        let found = find_muster(
            env(&[("HOME", "/home/p"), ("PATH", "/usr/bin:/bin")]),
            |path| programs.iter().any(|program| Path::new(program) == path),
        );
        assert_eq!(found, Some(PathBuf::from("/home/p/.cargo/bin/muster")));
    }

    #[test]
    fn muster_bin_comes_first_then_local_bin_then_cargo_then_path() {
        let all = |_: &Path| true;
        let vars = [
            ("MUSTER_BIN", "/opt/muster/muster"),
            ("HOME", "/home/p"),
            ("PATH", "/usr/bin"),
        ];
        assert_eq!(
            find_muster(env(&vars), all),
            Some(PathBuf::from("/opt/muster/muster"))
        );
        assert_eq!(
            find_muster(env(&vars[1..]), all),
            Some(PathBuf::from("/home/p/.local/bin/muster"))
        );
        assert_eq!(
            find_muster(env(&[("PATH", "relative:/usr/bin")]), all),
            Some(PathBuf::from("/usr/bin/muster")),
            "a relative folder on PATH is passed over"
        );
        assert_eq!(find_muster(env(&[]), all), None);
    }

    #[test]
    fn the_daemon_is_given_the_app_s_path_then_the_login_shell_s_then_muster_s_folder() {
        let path = daemon_path(
            Some(OsStr::new("/tests/bin:/usr/bin:/bin")),
            Some(OsStr::new("/home/p/.local/bin:/usr/bin:/opt/homebrew/bin")),
            Some(Path::new("/home/p/.cargo/bin")),
        );
        assert_eq!(
            path,
            "/tests/bin:/usr/bin:/bin:/home/p/.local/bin:/opt/homebrew/bin:/home/p/.cargo/bin"
        );
        assert_eq!(
            daemon_path(Some(OsStr::new("/usr/bin")), None, None),
            "/usr/bin"
        );
    }

    #[test]
    fn the_login_shell_s_path_is_read_between_its_markers() {
        let said = b"Welcome back!\n\n__MUSTER_PATH__/a:/b__MUSTER_PATH__\n";
        assert_eq!(between_markers(said).as_deref(), Some("/a:/b"));
        assert_eq!(between_markers(b"no markers"), None);
        assert_eq!(between_markers(b"__MUSTER_PATH____MUSTER_PATH__"), None);
    }

    #[test]
    fn a_login_shell_says_its_path() {
        let path = login_path(Path::new("/bin/sh")).expect("sh says its PATH");
        assert!(
            std::env::split_paths(&path)
                .any(|folder| folder == Path::new("/bin") || folder == Path::new("/usr/bin")),
            "{path:?}"
        );
    }
}

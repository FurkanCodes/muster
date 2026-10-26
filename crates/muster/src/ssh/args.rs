//! How Muster runs `ssh` (`ARC-27`): the one place its options are chosen,
//! the private folder its shared connections live in, how a command is
//! written for a shell there, and what ssh's complaints mean for the
//! person. The options given here win over the person's `~/.ssh/config`,
//! which still says the host, the user, the keys and any proxy.

use std::os::unix::fs::{
    DirBuilderExt as _, FileTypeExt as _, MetadataExt as _, PermissionsExt as _,
};
use std::path::{Path, PathBuf};

/// Where Muster looks for itself on a machine, in order: where its own
/// installer puts it, cargo's, Homebrew's on a Mac, the system's, then the
/// login's. Every script there sets it, so they all find the same `muster`.
pub(crate) const REMOTE_PATH: &str =
    "PATH=\"$HOME/.local/bin:$HOME/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:$PATH\"";

/// Whether ssh may ask the person anything.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum How {
    /// The person is at a terminal and waits on it: ssh may ask for a
    /// password, a key's passphrase or to trust a new host key.
    Interactive,
    /// Nobody can answer: the UI reconnecting, a script. ssh never asks,
    /// gives up within seconds, and a link that died is noticed soon.
    Background,
}

/// The arguments for `ssh` to `target`, before the remote command: a
/// terminal there only with `tty`; compression; keepalives; a time limit
/// to connect; never a question in the background; and the shared
/// connection in `control` where there is one.
pub(crate) fn ssh_args(how: How, tty: bool, target: &str, control: Option<&Path>) -> Vec<String> {
    let mut args: Vec<String> = vec![if tty { "-t" } else { "-T" }.to_owned(), "-C".to_owned()];
    let mut option = |text: String| {
        args.push("-o".to_owned());
        args.push(text);
    };
    let (every, misses) = match how {
        How::Interactive => (15, 4),
        How::Background => (5, 3),
    };
    option(format!("ServerAliveInterval={every}"));
    option(format!("ServerAliveCountMax={misses}"));
    option("ConnectTimeout=10".to_owned());
    if how == How::Background {
        option("BatchMode=yes".to_owned());
        option("NumberOfPasswordPrompts=0".to_owned());
        option("StrictHostKeyChecking=yes".to_owned());
    }
    if let Some(control) = control {
        option("ControlMaster=auto".to_owned());
        option("ControlPersist=600".to_owned());
        option(format!("ControlPath={}/%C", control.display()));
    }
    args.push("--".to_owned());
    args.push(target.to_owned());
    args
}

/// The folder Muster's shared ssh connections live in, made if need be:
/// short, as macOS takes a socket's path to about 104 bytes; the person's
/// alone (theirs, 0700, no symlink). `None` where it cannot be trusted, and
/// ssh then connects on its own each time.
pub(crate) fn control_dir() -> Option<PathBuf> {
    let uid = nix::unistd::getuid().as_raw();
    let base = PathBuf::from(format!("/tmp/muster-{uid}"));
    let dir = base.join("ssh");
    for folder in [&base, &dir] {
        let _ = std::fs::DirBuilder::new().mode(0o700).create(folder);
        if !private_dir(folder, uid) {
            return None;
        }
    }
    clear_stale(&dir);
    Some(dir)
}

/// Whether `folder` is a folder (not a link to one) of `uid`'s that nobody
/// else can enter.
fn private_dir(folder: &Path, uid: u32) -> bool {
    std::fs::symlink_metadata(folder).is_ok_and(|meta| {
        // No bit for the group or others: the six lowest are clear.
        meta.file_type().is_dir()
            && meta.uid() == uid
            && meta.permissions().mode().trailing_zeros() >= 6
    })
}

/// Removes the shared connections whose ssh has gone, so a socket left
/// behind never stands in the way of the next.
fn clear_stale(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let is_socket = entry.file_type().is_ok_and(|kind| kind.is_socket());
        if is_socket && std::os::unix::net::UnixStream::connect(entry.path()).is_err() {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// `text` in single quotes, as every shell reads it whole.
pub(crate) fn quoted(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

/// The command ssh runs there with no words: the person's own login
/// shell, as `ssh MACHINE` opens one, with Muster's `PATH` (so `muster`
/// and an agent in `~/.local/bin` are found), in `dir` where given.
pub(crate) fn login_shell(dir: Option<&str>) -> String {
    let script = format!(
        "{REMOTE_PATH}; {}exec \"${{SHELL:-/bin/sh}}\" -l",
        in_dir(dir)
    );
    format!("sh -c {}", quoted(&script))
}

/// The command ssh runs there for the person's `words`, as ssh would run
/// them (joined, read by a shell, so a pipe is a pipe), with Muster's
/// `PATH` (an agent installed in `~/.local/bin` is found), in `dir` where
/// given.
pub(crate) fn shell_command(dir: Option<&str>, words: &[String]) -> String {
    let script = format!("{REMOTE_PATH}; {}{}", in_dir(dir), words.join(" "));
    format!("sh -c {}", quoted(&script))
}

/// `cd` to `dir` there, then: `~/` is that machine's home.
fn in_dir(dir: Option<&str>) -> String {
    match dir {
        Some(dir) => match dir.strip_prefix("~/") {
            Some(rest) => format!("cd \"$HOME\"/{} && ", quoted(rest)),
            None => format!("cd {} && ", quoted(dir)),
        },
        None => String::new(),
    }
}

/// The user ssh signs in to `target` as, where it names none: what the
/// person's ssh settings say for the host, else their own name here
/// (`ssh -G`, which connects to nothing). `None` where the target names
/// one, or ssh cannot say.
pub(crate) fn user_for(target: &str) -> Option<String> {
    if target.contains('@') {
        return None;
    }
    let said = std::process::Command::new("ssh")
        .args(["-G", "--", target])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    String::from_utf8_lossy(&said.stdout)
        .lines()
        .find_map(|line| line.strip_prefix("user "))
        .map(|user| user.trim().to_owned())
        .filter(|user| !user.is_empty())
}

/// What ssh's complaint about reaching `machine` means, and what the person
/// does about it; `None` where it says nothing known.
pub(crate) fn hint(stderr: &str, machine: &str) -> Option<String> {
    let said = stderr.to_lowercase();
    let first = |words: &[&str]| words.iter().any(|word| said.contains(word));
    if first(&[
        "host key verification failed",
        "remote host identification has changed",
    ]) {
        return Some(format!(
            "ssh does not trust {machine}'s host key yet: connect once with `muster ssh \
             {machine}` (or `ssh` to it) and accept it"
        ));
    }
    if first(&["permission denied", "too many authentication failures"]) {
        return Some(format!(
            "ssh could not sign in to {machine} without asking: check the user name \
             (`you@host`), connect once with `muster ssh {machine}`, or add your key to your \
             agent (`ssh-add`)"
        ));
    }
    if first(&[
        "could not resolve",
        "name or service not known",
        "timed out",
        "connection refused",
        "no route to host",
        "network is unreachable",
    ]) {
        return Some(format!(
            "{machine} cannot be reached: is it on, and reachable from here (`ssh {machine}`)?"
        ));
    }
    if first(&[
        "muster: not found",
        "muster: command not found",
        "no such file",
    ]) {
        return Some(format!(
            "Muster is not on {machine}: `muster ssh {machine} --install` installs it"
        ));
    }
    if first(&[
        "unrecognized subcommand 'bridge'",
        "unexpected argument 'bridge'",
    ]) {
        return Some(format!(
            "Muster on {machine} is older than this one: `muster ssh {machine} --install` brings \
             it up"
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_background_never_asks_and_notices_a_dead_link_arc_27() {
        let control = Path::new("/tmp/muster-501/ssh");
        let background = ssh_args(How::Background, false, "you@vm", Some(control)).join(" ");
        for option in [
            "-T",
            "-C",
            "BatchMode=yes",
            "NumberOfPasswordPrompts=0",
            "StrictHostKeyChecking=yes",
            "ServerAliveInterval=5",
            "ConnectTimeout=10",
            "ControlMaster=auto",
            "ControlPath=/tmp/muster-501/ssh/%C",
        ] {
            assert!(background.contains(option), "{option}: {background}");
        }
        assert!(background.ends_with("-- you@vm"), "{background}");
        let asked = ssh_args(How::Interactive, true, "you@vm", None).join(" ");
        assert!(asked.starts_with("-t -C"), "{asked}");
        assert!(!asked.contains("BatchMode"), "{asked}");
        assert!(!asked.contains("Control"), "{asked}");
        assert!(asked.contains("ServerAliveInterval=15"), "{asked}");
    }

    #[test]
    fn the_control_folder_is_the_person_s_alone() {
        let dir = tempfile::tempdir().unwrap();
        let uid = nix::unistd::getuid().as_raw();
        let mine = dir.path().join("mine");
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&mine)
            .unwrap();
        assert!(private_dir(&mine, uid));
        let open = dir.path().join("open");
        std::fs::DirBuilder::new()
            .mode(0o755)
            .create(&open)
            .unwrap();
        assert!(!private_dir(&open, uid), "others can enter it");
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&mine, &link).unwrap();
        assert!(!private_dir(&link, uid), "a link to one is not one");
        assert!(!private_dir(&mine, uid + 1), "someone else's");
    }

    #[test]
    fn a_shared_connection_whose_ssh_went_is_cleared() {
        let dir = tempfile::tempdir().unwrap();
        let gone = dir.path().join("gone");
        drop(std::os::unix::net::UnixListener::bind(&gone).unwrap());
        let alive = dir.path().join("alive");
        let _listening = std::os::unix::net::UnixListener::bind(&alive).unwrap();
        clear_stale(dir.path());
        assert!(!gone.exists());
        assert!(alive.exists());
    }

    #[test]
    fn no_words_open_the_person_s_login_shell_there_with_muster_s_path_arc_27() {
        let shell = login_shell(None);
        assert!(shell.starts_with("sh -c '"), "{shell}");
        assert!(shell.contains(".local/bin"), "{shell}");
        assert!(
            shell.ends_with("; exec \"${SHELL:-/bin/sh}\" -l'"),
            "{shell}"
        );
        let in_dir = login_shell(Some("~/code it"));
        assert!(
            in_dir.contains("cd \"$HOME\"/'\\''code it'\\'' && exec "),
            "{in_dir}"
        );
    }

    #[test]
    fn the_person_s_command_there_runs_as_ssh_runs_one_with_muster_s_path() {
        let command = shell_command(None, &["claude".into(), "auth".into(), "login".into()]);
        assert!(command.starts_with("sh -c '"), "{command}");
        assert!(command.contains(".local/bin"), "{command}");
        assert!(command.ends_with("; claude auth login'"), "{command}");
        let piped = shell_command(Some("~/app"), &["git log | head".into()]);
        assert!(
            piped.contains("cd \"$HOME\"/'\\''app'\\'' && git log | head"),
            "{piped}"
        );
    }

    #[test]
    fn ssh_s_complaints_are_said_with_what_to_do() {
        let hint = |said| hint(said, "vm").unwrap_or_default();
        assert!(hint("Host key verification failed.").contains("accept it"));
        assert!(hint("you@vm: Permission denied (publickey).").contains("ssh-add"));
        assert!(hint("ssh: connect to host vm port 22: Operation timed out").contains("reachable"));
        assert!(hint("sh: 1: exec: muster: not found").contains("--install"));
        assert!(hint("error: unrecognized subcommand 'bridge'").contains("older"));
        assert_eq!(super::hint("something new", "vm"), None);
    }
}

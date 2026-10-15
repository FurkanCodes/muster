//! `muster reload`: moves the running daemon onto the muster installed
//! now, in place, every session running on (`ARCHITECTURE.md` §4), and
//! says what the daemon reported.

use std::path::Path;
use std::process::ExitCode;

use muster_client::ClientError;

use crate::commands::{CLIENT, count, shown};

/// The first muster whose daemon restarts in place on SIGUSR1; an older one
/// would end on it, taking its sessions with it.
const RELOADS_FROM: [u64; 3] = [0, 2, 0];

/// Whether a daemon of `version` restarts in place.
pub(crate) fn reloads(version: &str) -> bool {
    let parts: Vec<u64> = version
        .split('.')
        .map(|part| {
            let digits: String = part.chars().take_while(char::is_ascii_digit).collect();
            digits.parse().unwrap_or(0)
        })
        .collect();
    parts.len() >= 3 && [parts[0], parts[1], parts[2]] >= RELOADS_FROM
}

/// `muster reload`: moves the running daemon onto the muster installed now,
/// in place, every session running on; says what the daemon reported.
pub(crate) async fn reload_daemon(socket: &Path, runtime_dir: &Path) -> ExitCode {
    let (pid, before) = match muster_client::daemon_identity(socket, CLIENT).await {
        Ok(found) => found,
        Err(ClientError::Unreachable { .. }) => {
            println!("The daemon is not running; `muster` starts it on the version installed now.");
            return ExitCode::SUCCESS;
        }
        Err(error) => return crate::fail(error),
    };
    if !reloads(&before) {
        return crate::fail(format!(
            "the running daemon is muster {before}, from before restarts in place. Stop it once \
             with `muster daemon stop` (shells in sessions end; agents resume their conversations \
             when it starts again); from then on `muster reload` keeps every session running"
        ));
    }
    let Ok(raw) = i32::try_from(pid) else {
        return crate::fail(format!("the daemon reported an impossible pid {pid}"));
    };
    let log = runtime_dir.join("daemon.log");
    let logged = std::fs::metadata(&log).map_or(0, |metadata| metadata.len());
    let process = nix::unistd::Pid::from_raw(raw);
    if let Err(error) = nix::sys::signal::kill(process, nix::sys::signal::Signal::SIGUSR1) {
        return crate::fail(format!("could not reach the daemon (pid {pid}): {error}"));
    }
    // It stops answering as it starts over; whoever answers next is the
    // daemon after: the new version, or this one carrying on.
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    let deadline = crate::instant_now() + std::time::Duration::from_secs(30);
    let after = loop {
        let asked = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            muster_client::daemon_identity(socket, CLIENT),
        )
        .await;
        match asked {
            Ok(Ok((answering, version))) if answering == pid => break version,
            Ok(Ok((other, _))) => {
                return crate::fail(format!(
                    "another daemon (pid {other}) answers now, not the one restarting (pid {pid}); see {}",
                    log.display()
                ));
            }
            _ if crate::instant_now() >= deadline => {
                return crate::fail(format!(
                    "the daemon (pid {pid}) has not answered for 30 seconds; see {}",
                    log.display()
                ));
            }
            _ => tokio::time::sleep(std::time::Duration::from_millis(200)).await,
        }
    };
    // What it said as it went, if it logs where the UI has it log.
    let said = read_from(&log, logged);
    if let Some(reason) = said
        .lines()
        .find(|line| line.contains("could not restart in place"))
    {
        let reason = reason.split("reason=").nth(1).unwrap_or(reason);
        return crate::fail(format!(
            "the daemon could not move onto the new version and carries on as muster {after}: {}",
            shown(reason.trim())
        ));
    }
    let kept = said
        .lines()
        .filter(|line| line.contains("taking over from the daemon's previous version"))
        .find_map(|line| {
            let digits: String = line
                .split("sessions=")
                .nth(1)?
                .chars()
                .take_while(char::is_ascii_digit)
                .collect();
            digits.parse::<usize>().ok()
        });
    let was = if before == after {
        String::new()
    } else {
        format!(" (it was {})", shown(&before))
    };
    match kept {
        Some(sessions) => println!(
            "The daemon is now muster {}{was}; {} kept running.",
            shown(&after),
            count(sessions, "session")
        ),
        None => println!(
            "The daemon is now muster {}{was}, its sessions running on.",
            shown(&after)
        ),
    }
    ExitCode::SUCCESS
}

/// What `log` holds past `offset` (all of it if it has since been cut),
/// its terminal colours taken out.
fn read_from(log: &Path, offset: u64) -> String {
    let Ok(bytes) = std::fs::read(log) else {
        return String::new();
    };
    let start = usize::try_from(offset)
        .ok()
        .filter(|start| *start <= bytes.len())
        .unwrap_or(0);
    let text = String::from_utf8_lossy(&bytes[start..]);
    let mut plain = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' && chars.peek() == Some(&'[') {
            for next in chars.by_ref() {
                if next.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            plain.push(c);
        }
    }
    plain
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_daemons_from_0_2_0_restart_in_place() {
        for (version, can) in [
            ("0.1.1", false),
            ("0.1.0", false),
            ("0.2.0", true),
            ("0.10.3", true),
            ("1.0.0-rc.1", true),
            ("garbage", false),
            ("", false),
        ] {
            assert_eq!(reloads(version), can, "{version}");
        }
    }

    #[test]
    fn the_log_is_read_past_where_it_was_without_colours() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("daemon.log");
        std::fs::write(&log, "old line\n").unwrap();
        let offset = std::fs::metadata(&log).unwrap().len();
        let mut text = std::fs::read_to_string(&log).unwrap();
        text.push_str("\u{1b}[2mtime\u{1b}[0m taking over from the daemon's previous version \u{1b}[3msessions\u{1b}[0m\u{1b}[2m=\u{1b}[0m3\n");
        std::fs::write(&log, text).unwrap();
        let read = read_from(&log, offset);
        assert!(!read.contains("old line"));
        assert!(read.contains("previous version sessions=3"), "{read}");
        // A log since cut is read from its start.
        assert!(read_from(&log, 10_000).contains("sessions=3"));
    }
}

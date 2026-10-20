//! Muster's notch (`ARC-22`): on a MacBook with a notch, the agents shown
//! at it while the person looks away from every Muster window. How many
//! wait and how many work sit level with the notch, in a slim side either
//! side of it, as the notch itself has no pixels; pointed at, it opens
//! downwards into the list of them, and a click on one shows it in the
//! Muster window looked at last.
//!
//! The notch is a client of the daemon like the UI: it learns everything
//! from the daemon's state and changes nothing but through its commands.
//! [`board`] (what to show, with [`trouble`] in the work), [`geometry`]
//! (where) and [`hover`] (when the list opens) are pure and tested
//! anywhere; `link` follows the daemon; `mac` draws through AppKit and
//! holds the crate's only `unsafe` code (`ARC-12`).

pub mod board;
pub mod geometry;
pub mod hover;
pub mod trouble;
// The link to the daemon and its copy of the state serve only the screen,
// which is a Mac's; the copy is tested anywhere.
#[cfg(target_os = "macos")]
mod link;
#[cfg(target_os = "macos")]
mod mac;
#[cfg(any(target_os = "macos", test))]
mod mirror;
#[cfg(target_os = "macos")]
mod news;

use std::path::Path;

/// Why the notch could not run.
#[derive(Debug, thiserror::Error)]
pub enum NotchError {
    #[error("the notch is only on a Mac")]
    NotAMac,
    #[error("the notch must run on the program's main thread")]
    NotMainThread,
    #[error("could not start the notch: {0}")]
    Io(#[from] std::io::Error),
}

/// Set on a notch started again for a daemon of another version, naming
/// that version, so a program that is no more that version than this one
/// is not started over and over.
pub const MOVED_FOR: &str = "MUSTER_NOTCH_FOR";

/// The lock a running notch holds, in the runtime directory, so only one
/// runs however many UIs start it.
const LOCK: &str = "notch.lock";

/// Runs the notch for the daemon at `socket` until it is turned off or
/// the daemon is gone. Returns at once where another notch runs, or no
/// screen has a notch. Must be called on the program's main thread, which
/// AppKit takes; `relaunch` moves the notch onto the program installed
/// now when the daemon turns out to be another version, and returns only
/// if it could not.
pub fn run(
    socket: &Path,
    runtime_dir: &Path,
    relaunch: impl FnOnce(&str) -> std::io::Error + Send + 'static,
) -> Result<(), NotchError> {
    let lock = std::fs::File::options()
        .create(true)
        .write(true)
        .truncate(false)
        .open(runtime_dir.join(LOCK))?;
    match lock.try_lock() {
        Ok(()) => {}
        Err(std::fs::TryLockError::WouldBlock) => return Ok(()),
        Err(std::fs::TryLockError::Error(error)) => return Err(error.into()),
    }
    let outcome = show(socket, relaunch);
    // Held until the notch ends, however it ends.
    drop(lock);
    outcome
}

#[cfg(target_os = "macos")]
fn show(
    socket: &Path,
    relaunch: impl FnOnce(&str) -> std::io::Error + Send + 'static,
) -> Result<(), NotchError> {
    mac::run(socket.to_owned(), relaunch)
}

#[cfg(not(target_os = "macos"))]
fn show(
    _socket: &Path,
    _relaunch: impl FnOnce(&str) -> std::io::Error + Send + 'static,
) -> Result<(), NotchError> {
    Err(NotchError::NotAMac)
}

//! The only unsafe code in Muster (`ARC-12`): two operations on file
//! descriptors that no safe interface can express, both used only by the
//! daemon's restart in place.
//!
//! - [`duplicate`] borrows a descriptor that a library owns and does not
//!   hand out (portable-pty's terminal), to make a duplicate of Muster's own.
//! - [`inherit`] takes ownership of a descriptor the daemon's previous
//!   version left open across `exec` for this one, listed in the handoff.
//!
//! Everything else about descriptors (keeping one open across `exec`,
//! reading, polling) goes through safe interfaces on owned values.

#![expect(unsafe_code, reason = "ARC-12: the one audited module, as above")]

use std::io;
use std::os::fd::{BorrowedFd, FromRawFd, OwnedFd, RawFd};

use nix::fcntl::{FcntlArg, FdFlag, fcntl};

/// A duplicate of Muster's own of the descriptor `fd`, which the caller
/// holds open (through the value that owns it) for the length of the call.
pub(crate) fn duplicate(fd: RawFd) -> io::Result<OwnedFd> {
    // SAFETY: the caller keeps the owner of `fd` alive across this call, so
    // the descriptor is open; the borrow ends before the function returns.
    let borrowed = unsafe { BorrowedFd::borrow_raw(fd) };
    // Closed on exec from the start, so no program started meanwhile, by
    // another thread, can inherit it.
    let copy = fcntl(borrowed, FcntlArg::F_DUPFD_CLOEXEC(3)).map_err(io::Error::from)?;
    // SAFETY: F_DUPFD_CLOEXEC returned a new descriptor that nothing owns.
    Ok(unsafe { OwnedFd::from_raw_fd(copy) })
}

/// Ownership of the descriptor `fd`, left open across `exec` by the
/// daemon's previous version and listed in the handoff it left. Refused
/// for standard input, output and error and for a descriptor that is not
/// open. It is closed on `exec` again, so nothing the daemon starts from
/// now on inherits it.
pub(crate) fn inherit(fd: RawFd) -> io::Result<OwnedFd> {
    if fd < 3 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("descriptor {fd} is not one a handoff passes"),
        ));
    }
    // SAFETY: only a check that `fd` is open; the borrow ends at once.
    let open = fcntl(unsafe { BorrowedFd::borrow_raw(fd) }, FcntlArg::F_GETFD);
    open.map_err(|error| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("descriptor {fd} the handoff lists is not open: {error}"),
        )
    })?;
    // SAFETY: `fd` is open, and was left to this process by the daemon's
    // previous image as one it owned; the handoff names each descriptor once
    // and is read once, at start, before anything else could own it.
    let owned = unsafe { OwnedFd::from_raw_fd(fd) };
    fcntl(&owned, FcntlArg::F_SETFD(FdFlag::FD_CLOEXEC)).map_err(io::Error::from)?;
    Ok(owned)
}

/// Keeps the descriptor `value` owns open across `exec`, for the daemon's
/// next version.
pub(crate) fn keep_across_exec(value: &impl std::os::fd::AsFd) -> io::Result<()> {
    fcntl(value, FcntlArg::F_SETFD(FdFlag::empty()))
        .map(drop)
        .map_err(io::Error::from)
}

/// Closes the descriptor `value` owns on `exec` again: a handoff that did
/// not happen.
pub(crate) fn close_on_exec(value: &impl std::os::fd::AsFd) -> io::Result<()> {
    fcntl(value, FcntlArg::F_SETFD(FdFlag::FD_CLOEXEC))
        .map(drop)
        .map_err(io::Error::from)
}

#[cfg(test)]
mod tests {
    use std::os::fd::{AsFd, AsRawFd, IntoRawFd};

    use super::*;

    fn closes_on_exec(fd: &impl AsFd) -> bool {
        let flags = fcntl(fd, FcntlArg::F_GETFD).unwrap();
        FdFlag::from_bits_truncate(flags).contains(FdFlag::FD_CLOEXEC)
    }

    #[test]
    fn a_kept_descriptor_survives_exec_and_an_inherited_one_does_not_leak() {
        let file = tempfile::tempfile().unwrap();
        assert!(closes_on_exec(&file), "Rust opens files closed on exec");
        keep_across_exec(&file).unwrap();
        assert!(!closes_on_exec(&file));

        // What the next image does with it: owned once, closed on exec again.
        let raw = file.into_raw_fd();
        let inherited = inherit(raw).unwrap();
        assert_eq!(inherited.as_raw_fd(), raw);
        assert!(closes_on_exec(&inherited));
    }

    #[test]
    fn only_open_descriptors_beyond_stdio_are_inherited() {
        assert!(inherit(0).is_err());
        assert!(inherit(2).is_err());
        // Not a descriptor just closed: tests run side by side, and another
        // could be given its number meanwhile, which this one would then
        // own and close under it. No file is ever given this one.
        assert!(inherit(RawFd::MAX - 1).is_err());
    }

    #[test]
    fn a_duplicate_is_a_descriptor_of_its_own() {
        let file = tempfile::tempfile().unwrap();
        let copy = duplicate(file.as_raw_fd()).unwrap();
        assert_ne!(copy.as_raw_fd(), file.as_raw_fd());
        assert!(closes_on_exec(&copy));
    }
}

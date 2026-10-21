//! The commands the webview invokes (`src/ipc/tauri.ts` calls each by name).
//!
//! Each is thin: it hands the request to the link, which owns the one
//! connection to the daemon (`link.rs`, on `feature/desktop-link`). Until the
//! link lands they answer that the daemon is out of reach, which the webview
//! shows as it would a daemon that is down.

use muster_core::SessionId;
use muster_core::protocol::{Command, Reply};
use tauri::ipc::{Channel, InvokeResponseBody};

use crate::contract::{BridgeError, StateMessage};

/// Opens the link: the state streams on `on_state` from now on, starting
/// with how the link stands and, once connected, a patch with `reset`.
#[tauri::command]
pub(crate) async fn link_open(on_state: Channel<StateMessage>) -> Result<(), BridgeError> {
    let _ = on_state;
    Err(BridgeError::Unreachable)
}

/// Sends `command` to the daemon and returns its reply.
#[tauri::command]
pub(crate) async fn command(command: Command) -> Result<Reply, BridgeError> {
    let _ = command;
    Err(BridgeError::Unreachable)
}

/// Streams `session`'s terminal on `on_frame` (see [`crate::contract::frame`]),
/// from `from_seq` or everything the daemon still holds.
#[tauri::command]
pub(crate) async fn attach(
    session: SessionId,
    from_seq: Option<u64>,
    watching: bool,
    on_frame: Channel<InvokeResponseBody>,
) -> Result<(), BridgeError> {
    let _ = (session, from_seq, watching, on_frame);
    Err(BridgeError::Unreachable)
}

/// Stops streaming `session`'s terminal.
#[tauri::command]
pub(crate) async fn detach(session: SessionId) -> Result<(), BridgeError> {
    let _ = session;
    Err(BridgeError::Unreachable)
}

/// Keystrokes or pasted text for `session`'s program, as UTF-8.
#[tauri::command]
pub(crate) async fn input(session: SessionId, text: String) -> Result<(), BridgeError> {
    let _ = (session, text);
    Err(BridgeError::Unreachable)
}

/// The view of `session`'s terminal changed size.
#[tauri::command]
pub(crate) async fn resize(
    session: SessionId,
    cols: u16,
    rows: u16,
    width: Option<u16>,
    height: Option<u16>,
) -> Result<(), BridgeError> {
    let _ = (session, cols, rows, width, height);
    Err(BridgeError::Unreachable)
}

/// Whether the person is looking at an attached session.
#[tauri::command]
pub(crate) async fn watch(session: SessionId, watching: bool) -> Result<(), BridgeError> {
    let _ = (session, watching);
    Err(BridgeError::Unreachable)
}

/// Whether the person is looking at this window, and which session's
/// terminal is in focus in it.
#[tauri::command]
pub(crate) async fn looking(looking: bool, focused: Option<SessionId>) -> Result<(), BridgeError> {
    let _ = (looking, focused);
    Err(BridgeError::Unreachable)
}

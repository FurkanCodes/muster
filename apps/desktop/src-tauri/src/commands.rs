//! The commands the webview invokes (`src/ipc/tauri.ts` calls each by name).
//!
//! Each is thin: it hands the request to the link (`link.rs`), which owns
//! the one connection to the daemon. State streams on the channel
//! `link_open` is given, each terminal on the channel its `attach` is.

use muster_core::SessionId;
use muster_core::protocol::{Command, PixelSize, Reply, TermSize};
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{AppHandle, Manager, State};

use crate::contract::{BridgeError, StateMessage};
use crate::link::{FrameSink, Link, Looking, StateSink};

/// The webview's state channel. A session picked elsewhere (the notch, a
/// notification) brings the window to the front as it is opened there.
struct Webview {
    channel: Channel<StateMessage>,
    app: AppHandle,
}

impl StateSink for Webview {
    fn send(&self, message: StateMessage) {
        if matches!(message, StateMessage::Reveal { .. })
            && let Some(window) = self.app.get_webview_window("main")
        {
            let _ = window.unminimize();
            let _ = window.show();
            let _ = window.set_focus();
        }
        // A webview gone or loaded again takes nothing more on this channel;
        // the next `link_open` gives the link its new one.
        let _ = self.channel.send(message);
    }
}

/// A terminal's channel of raw frames (`contract::frame`).
struct Frames(Channel<InvokeResponseBody>);

impl FrameSink for Frames {
    fn send(&self, frame: Vec<u8>) -> bool {
        self.0.send(InvokeResponseBody::Raw(frame)).is_ok()
    }
}

/// Opens the link: the state streams on `on_state` from now on, starting
/// with how the link stands and, once connected, a patch with `reset`.
#[tauri::command]
pub(crate) async fn link_open(
    app: AppHandle,
    link: State<'_, Link>,
    on_state: Channel<StateMessage>,
) -> Result<(), BridgeError> {
    link.open(Webview {
        channel: on_state,
        app,
    })
}

/// Sends `command` to the daemon and returns its reply.
#[tauri::command]
pub(crate) async fn command(link: State<'_, Link>, command: Command) -> Result<Reply, BridgeError> {
    link.command(command).await
}

/// Streams `session`'s terminal on `on_frame` (see [`crate::contract::frame`]),
/// from `from_seq` or everything the daemon still holds.
#[tauri::command]
pub(crate) async fn attach(
    link: State<'_, Link>,
    session: SessionId,
    from_seq: Option<u64>,
    watching: bool,
    on_frame: Channel<InvokeResponseBody>,
) -> Result<(), BridgeError> {
    link.attach(session, from_seq, watching, Frames(on_frame))
}

/// Stops streaming `session`'s terminal.
#[tauri::command]
pub(crate) async fn detach(link: State<'_, Link>, session: SessionId) -> Result<(), BridgeError> {
    link.detach(session)
}

/// Keystrokes or pasted text for `session`'s program, as UTF-8.
#[tauri::command]
pub(crate) async fn input(
    link: State<'_, Link>,
    session: SessionId,
    text: String,
) -> Result<(), BridgeError> {
    link.input(session, text.into_bytes())
}

/// The view of `session`'s terminal changed size: `cols` by `rows`
/// cells, `width` by `height` pixels where the webview says.
#[tauri::command]
pub(crate) async fn resize(
    link: State<'_, Link>,
    session: SessionId,
    cols: u16,
    rows: u16,
    width: Option<u16>,
    height: Option<u16>,
) -> Result<(), BridgeError> {
    let pixels = width
        .zip(height)
        .map(|(width, height)| PixelSize { width, height });
    link.resize(session, TermSize { cols, rows }, pixels)
}

/// Whether the person is looking at an attached session.
#[tauri::command]
pub(crate) async fn watch(
    link: State<'_, Link>,
    session: SessionId,
    watching: bool,
) -> Result<(), BridgeError> {
    link.watch(session, watching)
}

/// Whether the person is looking at this window, which session's terminal
/// is in focus in it, and whether it can be seen at all.
#[tauri::command]
pub(crate) async fn looking(
    link: State<'_, Link>,
    looking: bool,
    focused: Option<SessionId>,
    seen: bool,
) -> Result<(), BridgeError> {
    link.looking(Looking {
        seen,
        looking,
        focused,
    })
}

/// Tries to reach the daemon now, starting it again if nothing answers:
/// the person's *Try now* while it is out of reach.
#[tauri::command]
pub(crate) async fn try_now(link: State<'_, Link>) -> Result<(), BridgeError> {
    link.try_now();
    Ok(())
}

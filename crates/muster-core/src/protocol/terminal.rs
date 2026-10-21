//! A terminal's modes and sizes, as a stream and a resize carry them.

use serde::{Deserialize, Serialize};

/// Input modes a program sets on its terminal, which decide how a client
/// turns key presses and pastes into bytes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct TerminalModes {
    /// The kitty keyboard protocol flags in effect; 0 means legacy keys.
    pub kitty_flags: u8,
    /// Pastes are wrapped in `ESC [200~` … `ESC [201~`.
    pub bracketed_paste: bool,
    /// Cursor keys send `ESC O A` rather than `ESC [ A` (DECCKM).
    pub application_cursor: bool,
    /// Which mouse events the program asked to be sent. While it asks for
    /// none, a client uses the mouse itself, to scroll back.
    #[serde(default)]
    pub mouse: MouseTracking,
    /// How mouse events are encoded for the program.
    #[serde(default)]
    pub mouse_encoding: MouseEncoding,
    /// The program draws on the alternate screen (a full-screen program
    /// such as vim or less), which keeps no scrollback.
    #[serde(default)]
    pub alternate_screen: bool,
}

/// The mouse events a program asked its terminal to report (xterm's modes
/// 9, 1000, 1002 and 1003).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum MouseTracking {
    #[default]
    Off,
    /// Button presses only.
    Press,
    /// Presses and releases, the wheel included.
    PressRelease,
    /// Also movement while a button is held.
    ButtonMotion,
    /// Also any movement.
    AnyMotion,
}

/// How reported mouse events are encoded (xterm's modes 1005 and 1006).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum MouseEncoding {
    /// `ESC [ M` and three bytes.
    #[default]
    Default,
    /// The same, with large coordinates as UTF-8.
    Utf8,
    /// `ESC [ < button ; column ; row M` or `m`.
    Sgr,
}

/// Why a terminal stream ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum DetachReason {
    /// The session's process exited; everything it wrote has been sent.
    Exited,
    /// The session is unknown, or its terminal is no longer held.
    NotRunning,
}

/// A terminal's size in character cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct TermSize {
    pub cols: u16,
    pub rows: u16,
}

impl Default for TermSize {
    fn default() -> Self {
        Self { cols: 80, rows: 24 }
    }
}

/// A terminal's size in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct PixelSize {
    pub width: u16,
    pub height: u16,
}

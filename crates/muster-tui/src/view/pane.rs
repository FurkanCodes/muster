//! What a panel's title offers to click (`PRODUCT.md` §6 Sessions): at its
//! right end, a placed terminal's `✕`, which takes it off the screen, its
//! session running on; and on the preview, while it shows a session, a
//! `◆`, which places that session where the preview is, and a `✕`, which
//! folds the preview away until the list selects another session not
//! placed. Where they are is one function, which the mouse reads too.

use muster_core::layout::Panel;
use ratatui::layout::Rect;

use crate::app::App;

/// A button in a panel's title.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Button {
    /// Place the preview's session where the preview is.
    Place,
    /// Take the panel off the screen.
    TakeOff,
}

impl Button {
    /// Its mark, as drawn.
    pub(crate) const fn mark(self) -> &'static str {
        match self {
            Self::Place => "◆",
            Self::TakeOff => "✕",
        }
    }
}

/// The cells each button takes: its mark with a space either side.
const WIDTH: u16 = 3;

/// The buttons `panel`'s title offers, left to right.
fn buttons(app: &App, panel: Panel) -> &'static [Button] {
    match panel {
        Panel::Placed(_) => &[Button::TakeOff],
        Panel::Session if app.preview_session().is_some() => &[Button::Place, Button::TakeOff],
        _ => &[],
    }
}

/// Each of `panel`'s buttons and where it is drawn in `area`, its frame:
/// along the top border's right end, before its corner. None where the
/// title is too short to keep room for its name.
pub(crate) fn button_areas(app: &App, panel: Panel, area: Rect) -> Vec<(Button, Rect)> {
    let buttons = buttons(app, panel);
    let width = WIDTH * u16::try_from(buttons.len()).unwrap_or(0);
    if area.height == 0 || area.width < width + 12 {
        return Vec::new();
    }
    let start = area.right() - 1 - width;
    buttons
        .iter()
        .zip(0u16..)
        .map(|(button, at)| (*button, Rect::new(start + at * WIDTH, area.y, WIDTH, 1)))
        .collect()
}

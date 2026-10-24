//! What floats over the screen, holding the keys while it is shown: a
//! dialog, the keys' help, a menu. Each is a box cleared of what is
//! under it, its rounded border in the accent, as what has the keys is.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Block, BorderType, Borders, Clear};

use super::theme::Theme;

/// A box `width` by `height` in the middle of `area`, no taller than it.
pub(super) fn centred(area: Rect, width: u16, height: u16) -> Rect {
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width: width.min(area.width),
        height: height.min(area.height),
    }
}

/// Draws the box at `rect`, titled `title` where there is one, and
/// returns what is inside its border.
pub(super) fn draw<'a>(
    frame: &mut Frame<'_>,
    rect: Rect,
    title: Option<impl Into<Line<'a>>>,
    theme: Theme,
) -> Rect {
    frame.render_widget(Clear, rect);
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent));
    if let Some(title) = title {
        block = block.title(title);
    }
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    inner
}

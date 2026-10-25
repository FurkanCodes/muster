//! A slim bar on a frame's right edge where what is in it is longer than
//! it shows (`PRODUCT.md` §6): a long list, the help, a diff, a terminal
//! scrolled back. Its thumb says how much shows and where; dragging it, or
//! a click on the track, goes there (`app::scrollbars`).

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;

use crate::app::App;
use crate::app::scrollbars::Extent;

/// The track down `frame`'s right edge, between its corners.
pub(crate) fn track(frame: Rect) -> Rect {
    Rect {
        x: frame.right().saturating_sub(1),
        y: frame.y + 1,
        width: 1,
        height: frame.height.saturating_sub(2),
    }
}

/// The thumb in `track` for `extent`: as tall as the share that shows,
/// at least a cell, as far down as what shows is.
pub(crate) fn thumb(track: Rect, extent: Extent) -> Rect {
    let rows = usize::from(track.height);
    let total = extent.total.max(1);
    let height = (extent.shown * rows / total).clamp(1, rows.max(1));
    let room = rows - height;
    let past = extent.total.saturating_sub(extent.shown).max(1);
    let down = extent.first.min(past) * room / past;
    Rect {
        y: track.y + u16::try_from(down).unwrap_or(0),
        height: u16::try_from(height).unwrap_or(1),
        ..track
    }
}

/// The first line shown once the thumb is put at `row` of `track`.
pub(crate) fn first_at(track: Rect, extent: Extent, row: u16) -> usize {
    let past = extent.total.saturating_sub(extent.shown);
    let along = usize::from(row.saturating_sub(track.y));
    let rows = usize::from(track.height.saturating_sub(1)).max(1);
    (along * past / rows).min(past)
}

/// Draws each bar on the screen over its frame's edge, its thumb lit
/// while it is dragged or under the pointer.
pub(super) fn draw_bars(frame: &mut Frame<'_>, app: &App) {
    let theme = super::theme::of(app);
    for (bar, track, extent) in app.bars() {
        let thumb = thumb(track, extent);
        let lit = app.dragging_bar() == Some(bar)
            || app.person.pointer.is_some_and(|at| track.contains(at));
        let style = if lit {
            Style::default().fg(theme.accent)
        } else {
            theme.quiet
        };
        let buffer = frame.buffer_mut();
        for y in thumb.y..thumb.bottom() {
            if let Some(cell) = buffer.cell_mut((thumb.x, y)) {
                cell.set_symbol("┃").set_style(style);
            }
        }
    }
}

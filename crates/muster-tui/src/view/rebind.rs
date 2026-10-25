//! Every action and its keys, over the screen, for the person to set them
//! (`app::rebind`): the one chosen lit, the person's own marked, and what
//! the keys do here at the foot.

use ratatui::Frame;
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use super::{panel, theme};
use crate::app::{App, RebindRow, Rebinding, Rebound};
use crate::cells;

/// The cells the keys' column takes.
const KEYS_WIDTH: usize = 18;

/// The hint at the dialog's foot while it waits for no key.
const HINT: &str =
    "↑↓ choose   ⏎ set its key   ⌫ back to Muster's   Shift-R every key back   esc close";

/// Where the dialog is in `area`, inside its frame, how many of its rows
/// show, and the first shown, scrolled so the chosen line shows, its
/// heading too where it can.
fn laid_out(app: &App, rebinding: &Rebinding, area: Rect) -> (Rect, Rect, usize, usize) {
    let area = area.inner(Margin::new(2, 1));
    let inner = area.inner(Margin::new(1, 1));
    let rows = app.rebind_rows();
    let chosen = app.rebinding_chosen(rebinding);
    let at = rows
        .iter()
        .position(|row| *row == RebindRow::Key(chosen.clone()))
        .unwrap_or(0);
    let height = usize::from(inner.height.saturating_sub(2)).max(1);
    (
        area,
        inner,
        height,
        at.saturating_sub(height.saturating_sub(2)),
    )
}

/// Where the dialog is in `area`, and where a click chooses an action's
/// line or presses a key its hint names (`ARC-32`).
pub(super) fn rebinding_presses(
    app: &App,
    rebinding: &Rebinding,
    area: Rect,
) -> (Rect, Vec<(Rect, super::dialogs::DialogPress)>) {
    use super::dialogs::DialogPress;
    let (area, inner, height, offset) = laid_out(app, rebinding, area);
    let rows = app.rebind_rows();
    let mut presses: Vec<(Rect, DialogPress)> = rows
        .iter()
        .enumerate()
        .skip(offset)
        .take(height)
        .zip(0u16..)
        .filter(|((_, row), _)| matches!(row, RebindRow::Key(_)))
        .map(|((index, _), line)| {
            (
                Rect::new(inner.x, inner.y + line, inner.width, 1),
                DialogPress::Row(index),
            )
        })
        .collect();
    if !rebinding.waiting {
        let row = inner.y + u16::try_from(height + 1).unwrap_or(0);
        presses.extend(
            super::dialogs::hint_marks(HINT)
                .into_iter()
                .map(|(from, width, key)| {
                    (
                        Rect::new(inner.x + from, row, width, 1),
                        DialogPress::Key(key),
                    )
                }),
        );
    }
    (area, presses)
}

pub(super) fn draw_rebinding(frame: &mut Frame<'_>, app: &App, rebinding: &Rebinding, area: Rect) {
    let (area, inner, height, offset) = laid_out(app, rebinding, area);
    frame.render_widget(Clear, area);
    let block = panel(app, " Keys ", true);
    frame.render_widget(block, area);
    let rows = app.rebind_rows();
    let chosen = app.rebinding_chosen(rebinding);
    let theme = theme::of(app);
    let mut lines: Vec<Line<'static>> = rows
        .iter()
        .skip(offset)
        .take(height)
        .map(|row| match row {
            RebindRow::Heading(heading) => {
                Line::from(Span::from(heading.clone()).fg(theme.accent).bold())
            }
            RebindRow::Key(rebound) => action_line(app, rebound, *rebound == chosen),
        })
        .collect();
    while lines.len() < height {
        lines.push(Line::from(""));
    }
    lines.push(Line::from(""));
    lines.push(if rebinding.waiting {
        Line::from(format!(
            "Press the key for: {}   (esc keeps it as it is)",
            app.rebound_what(&chosen)
        ))
        .fg(theme.accent)
        .bold()
    } else {
        Line::from(HINT).dim()
    });
    frame.render_widget(Paragraph::new(lines), inner);
}

/// An action's line: its keys, what it does, and whether the keys are the
/// person's own.
fn action_line(app: &App, rebound: &Rebound, chosen: bool) -> Line<'static> {
    let keys = format!("  {}", app.rebound_labels(rebound));
    let gap = KEYS_WIDTH.saturating_sub(cells::width(&keys)).max(1);
    let mut spans = vec![
        Span::from(keys).bold(),
        Span::raw(" ".repeat(gap)),
        Span::raw(app.rebound_what(rebound)),
    ];
    if app.rebound_is_set(rebound) {
        spans.push(Span::from("  · yours").dim());
    }
    let line = Line::from(spans);
    if chosen {
        line.style(Style::default().reversed())
    } else {
        line
    }
}

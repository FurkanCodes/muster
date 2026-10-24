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

pub(super) fn draw_rebinding(frame: &mut Frame<'_>, app: &App, rebinding: &Rebinding, area: Rect) {
    let area = area.inner(Margin::new(2, 1));
    frame.render_widget(Clear, area);
    let block = panel(app, " Keys ", true);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let rows = app.rebind_rows();
    let chosen = app.rebinding_chosen(rebinding);
    let at = rows
        .iter()
        .position(|row| *row == RebindRow::Key(chosen.clone()))
        .unwrap_or(0);
    let height = usize::from(inner.height.saturating_sub(2)).max(1);
    // Scrolled so the chosen line shows, its heading too where it can.
    let offset = at.saturating_sub(height.saturating_sub(2));
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
        Line::from(
            "↑↓ choose   ⏎ set its key   ⌫ back to Muster's   Shift-R every key back   esc close",
        )
        .dim()
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

//! The menu a right-click opens on a line: what can be done there, each
//! with its key, the one ↑↓ are on lit.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use super::theme::Theme;
use crate::app::Menu;
use crate::cells;

/// Where `menu` is drawn in `window`: just below the cell clicked, or
/// above it where there is no room below, and inside the window. The
/// mouse finds its items here (`app::menu`).
pub(crate) fn menu_area(menu: &Menu, window: Rect) -> Rect {
    let widest = menu
        .items
        .iter()
        .map(|item| cells::width(&item.key_name()) + 2 + cells::width(&item.what))
        .max()
        .unwrap_or(0);
    let width = u16::try_from(widest + 4)
        .unwrap_or(u16::MAX)
        .min(window.width);
    let height = u16::try_from(menu.items.len() + 2)
        .unwrap_or(u16::MAX)
        .min(window.height);
    let (column, row) = menu.at;
    let x = column.min(window.right().saturating_sub(width));
    let below = row.saturating_add(1);
    let y = if below.saturating_add(height) <= window.bottom() {
        below
    } else {
        row.saturating_sub(height).max(window.y)
    };
    Rect::new(x, y, width, height)
}

pub(super) fn draw_menu(frame: &mut Frame<'_>, menu: &Menu, window: Rect, theme: Theme) {
    let area = menu_area(menu, window);
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent));
    let lines: Vec<Line<'_>> = menu
        .items
        .iter()
        .enumerate()
        .map(|(at, item)| {
            let line = Line::from(vec![
                Span::from(format!(" {} ", item.key_name())).bold(),
                Span::raw(format!(" {} ", item.what)),
            ]);
            if at == menu.chosen {
                line.reversed()
            } else {
                line
            }
        })
        .collect();
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

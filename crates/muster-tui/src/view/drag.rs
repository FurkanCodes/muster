//! The panels as they move: where one dragged would land, lit; and the
//! one being moved with the keys, framed in the accent.

use muster_core::layout::Panel;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::{Block, Borders};

use super::{BrowseAreas, theme};
use crate::app::App;
use crate::app::drag::Drag;

pub(super) fn draw_moving(frame: &mut Frame<'_>, app: &App, areas: &BrowseAreas) {
    let theme = theme::of(app);
    if let Some(Drag::Panel {
        to: Some(place), ..
    }) = &app.drag
    {
        let landing = app.landing(*place);
        frame
            .buffer_mut()
            .set_style(landing, Style::default().bg(theme.tint));
        outline(frame, landing, " here ", Style::default().fg(theme.accent));
    }
    if let Some(arranging) = &app.arranging {
        let area = match arranging.panel {
            Panel::List => areas.list,
            Panel::Session => areas.preview,
            Panel::Changes => areas.changes,
            Panel::Shell => Rect::default(),
        };
        if !area.is_empty() {
            outline(
                frame,
                area,
                " moving ",
                Style::default().fg(theme.accent).bold(),
            );
        }
    }
}

fn outline(frame: &mut Frame<'_>, area: Rect, title: &str, style: Style) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(style)
        .title(title.to_owned())
        .title_style(style);
    frame.render_widget(block, area);
}

//! The panels as they move: what the pointer could take hold of, and the
//! line being dragged, in the accent on its shade, apart from a border
//! that is the accent already; where a panel dragged would land,
//! lit; and the one being moved with the keys, framed in the accent.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::{Block, Borders};

use super::{BrowseAreas, theme};
use crate::app::App;
use crate::app::drag::Drag;
use crate::app::grab::Grab;

pub(super) fn draw_moving(frame: &mut Frame<'_>, app: &App, areas: &BrowseAreas) {
    let theme = theme::of(app);
    let held = match &app.panels.hover {
        Some(Grab::Line(path)) => areas
            .dividers
            .iter()
            .find(|divider| divider.path == *path)
            .map(crate::layout::Divider::line),
        Some(Grab::Title(panel)) => Some(Rect {
            height: 1,
            ..areas.of(*panel)
        }),
        None => None,
    };
    if let Some(held) = held.filter(|held| !held.is_empty()) {
        frame
            .buffer_mut()
            .set_style(held, Style::default().fg(theme.accent).bg(theme.lit).bold());
    }
    // A project carried over another's lines: where it lands, lit.
    if let Some(Drag::Project {
        project,
        onto: Some(onto),
        ..
    }) = &app.panels.drag
        && onto != project
        && let Some(line) = app.project_line_area(*onto)
    {
        frame
            .buffer_mut()
            .set_style(line, Style::default().bg(theme.tint));
        frame
            .buffer_mut()
            .set_style(line, Style::default().fg(theme.accent).bold());
    }
    if let Some(Drag::Panel {
        to: Some(place), ..
    }) = &app.panels.drag
    {
        let landing = app.landing(*place);
        frame
            .buffer_mut()
            .set_style(landing, Style::default().bg(theme.tint));
        outline(frame, landing, " here ", Style::default().fg(theme.accent));
    }
    if let Some(arranging) = &app.panels.arranging {
        let area = areas.of(arranging.panel);
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

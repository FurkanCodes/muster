//! The panels as they move: what the pointer could take hold of, and the
//! line being dragged, in the accent on its shade, apart from a border
//! that is the accent already; where a panel dragged would land,
//! lit; and the one being moved with the keys, framed in the accent.

use muster_core::layout::Panel;
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
    let held = match &app.hover {
        Some(Grab::Line(path)) => areas
            .dividers
            .iter()
            .find(|divider| divider.path == *path)
            .map(crate::layout::Divider::line),
        Some(Grab::Title(panel)) => Some(Rect {
            height: 1,
            ..area_of(areas, *panel)
        }),
        None => None,
    };
    if let Some(held) = held.filter(|held| !held.is_empty()) {
        frame
            .buffer_mut()
            .set_style(held, Style::default().fg(theme.accent).bg(theme.lit).bold());
    }
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
        let area = area_of(areas, arranging.panel);
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

fn area_of(areas: &BrowseAreas, panel: Panel) -> Rect {
    match panel {
        Panel::List => areas.list,
        Panel::Session => areas.preview,
        Panel::Changes => areas.changes,
        Panel::Shell => areas.shell,
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

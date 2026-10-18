//! The help over the screen: every key of the screen shown, under their
//! headings, narrowed to what is typed.

use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use crate::app::{App, Help, HelpLine, View};

/// The cells a key's column takes, before what it does.
const KEYS_WIDTH: usize = 22;

pub(super) fn draw_help(frame: &mut Frame<'_>, app: &App, help: &Help, window: Rect) {
    let width = window.width.saturating_sub(4).min(76);
    let height = window.height.saturating_sub(2).min(30);
    let area = Rect::new(
        window.x + window.width.saturating_sub(width) / 2,
        window.y + window.height.saturating_sub(height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, area);
    let screen = if help.over_changes {
        "Changes"
    } else {
        match app.view {
            View::Sessions => "Sessions",
            View::NeedsYou => "Needs you",
            View::Tickets => "Tickets",
            View::Settings => "Settings",
        }
    };
    let block = Block::default()
        .title(format!(" Keys · {screen} "))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().cyan());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height < 3 {
        return;
    }
    let typed = help.filter.text();
    let find = Line::from(vec![
        Span::from("Find ").dim(),
        Span::from(typed.to_owned()).bold(),
    ]);
    let mut lines = vec![find, Line::from("")];
    let shown = help.lines(app.view);
    if shown.is_empty() {
        lines.push(Line::from(format!("No key says “{typed}”.")).dim());
    }
    let room = usize::from(inner.height.saturating_sub(3));
    let width = usize::from(inner.width);
    for line in shown.iter().skip(help.offset) {
        let rows = help_rows(line, width);
        if lines.len() - 2 + rows.len() > room {
            break;
        }
        lines.extend(rows);
    }
    let [body, hint] = ratatui::layout::Layout::vertical([
        ratatui::layout::Constraint::Min(0),
        ratatui::layout::Constraint::Length(1),
    ])
    .areas(inner);
    frame.render_widget(Paragraph::new(lines), body);
    frame.render_widget(
        Line::from("type to narrow   ↑↓ scroll   esc close").dim(),
        hint,
    );
    let cursor = u16::try_from(5 + help.filter.before_cursor().chars().count()).unwrap_or(0);
    frame.set_cursor_position(Position::new(inner.x + cursor.min(inner.width), inner.y));
}

/// The rows `line` takes `width` cells wide: what a key does wraps under
/// itself, clear of the keys' column.
fn help_rows(line: &HelpLine, width: usize) -> Vec<Line<'static>> {
    match line {
        HelpLine::Heading(heading) => vec![Line::from(Span::from(*heading).cyan().bold())],
        HelpLine::Key((keys, what)) => {
            let keys = format!("  {keys}");
            let gap = KEYS_WIDTH.saturating_sub(keys.chars().count()).max(1);
            let indent = keys.chars().count() + gap;
            let mut rows = wrapped(what, width.saturating_sub(indent).max(12)).into_iter();
            let first = rows.next().unwrap_or_default();
            let mut lines = vec![Line::from(vec![
                Span::from(keys).bold(),
                Span::raw(" ".repeat(gap)),
                Span::raw(first),
            ])];
            lines.extend(rows.map(|row| Line::from(format!("{}{row}", " ".repeat(indent)))));
            lines
        }
    }
}

/// `text` in lines of at most `width` characters, broken between words.
fn wrapped(text: &str, width: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    for word in text.split(' ') {
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    lines.push(line);
    lines
}

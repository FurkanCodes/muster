//! A workspace's changes in the pane beside the list: the listing, or one
//! file's diff. Paths and subjects come from the repository, so they are
//! drawn on one line, cleaned; a diff is cleaned by the daemon.

use muster_core::changes::{ChangePart, ChangeSummary, FileChange, LineCounts};
use muster_core::text::one_line;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::{look, panel, shorten};
use crate::app::{App, ChangeRow, ChangesPane, DiffPane, change_rows};

pub(super) fn draw_changes(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let Some(pane) = &app.changes else {
        return;
    };
    match &pane.diff {
        Some(diff) => draw_diff(frame, diff, area),
        None => draw_listing(frame, app, pane, area),
    }
}

fn draw_listing(frame: &mut Frame<'_>, app: &App, pane: &ChangesPane, area: Rect) {
    let branch = app.model.branch(pane.workspace).unwrap_or("?");
    let block = panel(format!(" Changes · {} ", one_line(branch)), true);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let width = usize::from(inner.width);
    let lines: Vec<Line<'static>> = match &pane.listing {
        None => vec![Line::from(" Asking git…").dim()],
        Some(Err(error)) => vec![
            Line::from(format!(" {}", one_line(error))).red(),
            Line::from(" r asks again.").dim(),
        ],
        Some(Ok(changes)) => {
            let selected = pane.selected_row();
            change_rows(changes)
                .iter()
                .enumerate()
                .skip(pane.offset)
                .take(usize::from(inner.height))
                .map(|(at, row)| row_line(row, width, selected == Some(at)))
                .collect()
        }
    };
    frame.render_widget(Paragraph::new(lines), inner);
}

/// A line of the listing, `width` cells wide.
fn row_line(row: &ChangeRow<'_>, width: usize, selected: bool) -> Line<'static> {
    match row {
        ChangeRow::Heading(text) => Line::from(format!(" {text}")).bold(),
        ChangeRow::Note(text) => Line::from(format!(" {text}")).dim(),
        ChangeRow::Commit(commit) => Line::from(vec![
            Span::raw("   "),
            Span::styled(one_line(&commit.id), Style::default().fg(Color::Yellow)),
            Span::raw(" "),
            Span::raw(shorten(
                &commit.subject,
                width.saturating_sub(commit.id.chars().count() + 4),
            )),
        ]),
        ChangeRow::File(_, file) => {
            let line = file_line(file, width);
            if selected {
                line.style(look::selected_style(true))
            } else {
                line
            }
        }
    }
}

/// `  M  src/view.rs        +12 −3`: its kind, where it is (and was), and
/// its lines at the end.
fn file_line(file: &FileChange, width: usize) -> Line<'static> {
    let counts = file
        .lines
        .map_or_else(|| vec![Span::from("binary").dim()], counts);
    let counts_width: usize = counts.iter().map(Span::width).sum();
    let name = match &file.from {
        Some(from) => format!("{} → {}", one_line(from), one_line(&file.path)),
        None => one_line(&file.path),
    };
    let lead = 6;
    let room = width.saturating_sub(lead + counts_width + 2);
    let name = shorten_start(&name, room.max(4));
    let gap = width
        .saturating_sub(lead + name.chars().count() + counts_width + 1)
        .max(1);
    let mut spans = vec![
        Span::raw("   "),
        Span::styled(file.kind.letter().to_string(), kind_style(file)),
        Span::raw("  "),
        Span::raw(name),
        Span::raw(" ".repeat(gap)),
    ];
    spans.extend(counts);
    Line::from(spans)
}

fn kind_style(file: &FileChange) -> Style {
    use muster_core::ChangeKind;
    match file.kind {
        ChangeKind::Added | ChangeKind::Untracked | ChangeKind::Copied => {
            Style::default().fg(Color::Green)
        }
        ChangeKind::Deleted | ChangeKind::Conflicted => Style::default().fg(Color::Red),
        ChangeKind::Renamed | ChangeKind::TypeChanged => Style::default().fg(Color::Cyan),
        ChangeKind::Modified => Style::default().fg(Color::Yellow),
    }
}

/// `+12 −3`, the added in green and the removed in red; a side with
/// nothing is left out, and nothing on either side is `±0`.
pub(super) fn counts(lines: LineCounts) -> Vec<Span<'static>> {
    let added = Span::styled(
        format!("+{}", lines.added),
        Style::default().fg(Color::Green),
    );
    let removed = Span::styled(
        format!("−{}", lines.removed),
        Style::default().fg(Color::Red),
    );
    match (lines.added, lines.removed) {
        (0, 0) => vec![Span::from("±0").dim()],
        (_, 0) => vec![added],
        (0, _) => vec![removed],
        _ => vec![added, Span::raw(" "), removed],
    }
}

/// What a workspace's change comes to, for its line in the list: its
/// lines, or how many files where none are counted.
pub(super) fn summary(summary: &ChangeSummary) -> Vec<Span<'static>> {
    if summary.added == 0 && summary.removed == 0 {
        return vec![Span::from(summary.words()).dim()];
    }
    counts(LineCounts {
        added: summary.added,
        removed: summary.removed,
    })
}

/// `text`, cut to `max` characters from its start, as a path's end says
/// most: `…/view/changes.rs`.
fn shorten_start(text: &str, max: usize) -> String {
    let count = text.chars().count();
    if count <= max {
        return text.to_owned();
    }
    let kept: String = text.chars().skip(count - max.saturating_sub(1)).collect();
    format!("…{kept}")
}

fn draw_diff(frame: &mut Frame<'_>, diff: &DiffPane, area: Rect) {
    let part = match diff.part {
        ChangePart::Uncommitted => "not committed",
        ChangePart::Branch => "on the branch",
    };
    let block = panel(format!(" {} · {part} ", one_line(&diff.path)), true);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let lines: Vec<Line<'static>> = match &diff.text {
        None => vec![Line::from(" Asking git…").dim()],
        Some(Err(error)) => vec![Line::from(format!(" {}", one_line(error))).red()],
        Some(Ok(found)) if found.text.is_empty() => {
            vec![Line::from(" Git shows no difference in its text.").dim()]
        }
        Some(Ok(found)) => {
            let mut lines: Vec<Line<'static>> = found
                .text
                .lines()
                .skip(diff.scroll)
                .take(usize::from(inner.height))
                .map(diff_line)
                .collect();
            let shown = diff.scroll + lines.len();
            if found.cut
                && shown >= found.text.lines().count()
                && lines.len() < usize::from(inner.height)
            {
                lines.push(Line::from(" … cut here: the rest is longer than Muster shows.").dim());
            }
            lines
        }
    };
    frame.render_widget(Paragraph::new(lines), inner);
}

/// A line of a diff in its colours: added green, removed red, a hunk's
/// place cyan, git's own headers dim. Tabs are drawn as spaces, as a
/// terminal cell holds one character.
fn diff_line(line: &str) -> Line<'static> {
    let text = line.replace('\t', "    ");
    let style = if line.starts_with("+++") || line.starts_with("---") {
        Style::default().bold()
    } else if line.starts_with('+') {
        Style::default().fg(Color::Green)
    } else if line.starts_with('-') {
        Style::default().fg(Color::Red)
    } else if line.starts_with("@@") {
        Style::default().fg(Color::Cyan)
    } else if line.starts_with("diff ")
        || line.starts_with("index ")
        || line.starts_with("new file")
        || line.starts_with("deleted file")
        || line.starts_with("similarity")
        || line.starts_with("rename ")
        || line.starts_with('\\')
    {
        Style::default().dim()
    } else {
        Style::default()
    };
    Line::styled(text, style)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(spans: &[Span<'_>]) -> String {
        spans.iter().map(|span| span.content.as_ref()).collect()
    }

    #[test]
    fn counts_leave_out_an_empty_side() {
        let lines = |added, removed| text(&counts(LineCounts { added, removed }));
        assert_eq!(lines(12, 3), "+12 −3");
        assert_eq!(lines(12, 0), "+12");
        assert_eq!(lines(0, 3), "−3");
        assert_eq!(lines(0, 0), "±0");
    }

    #[test]
    fn a_long_path_keeps_its_end() {
        assert_eq!(shorten_start("src/view/changes.rs", 12), "…/changes.rs");
        assert_eq!(shorten_start("a.rs", 12), "a.rs");
    }
}

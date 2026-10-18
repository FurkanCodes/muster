//! A workspace's changes in the pane beside the list: the listing with the
//! workspace's timeline, or one file's or entry's diff. Paths and subjects
//! come from the repository, so they are drawn on one line, cleaned; a
//! diff is cleaned by the daemon.

use muster_core::changes::{ChangePart, ChangeSummary, FileChange, LineCounts};
use muster_core::text::one_line;
use muster_core::timeline::{Entry, Timeline};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::{look, panel, shorten};
use crate::app::{App, ChangeRow, ChangesPane, DiffPane, Pick};

pub(super) fn draw_changes(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let Some(pane) = &app.changes else {
        return;
    };
    match &pane.diff {
        Some(diff) => draw_diff(frame, pane, diff, area),
        None => draw_listing(frame, app, pane, area),
    }
}

fn draw_listing(frame: &mut Frame<'_>, app: &App, pane: &ChangesPane, area: Rect) {
    let branch = app.model.branch(pane.workspace).unwrap_or("?");
    let block = panel(format!(" Changes · {} ", one_line(branch)), true);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let width = usize::from(inner.width);
    let selected = pane.selected_row();
    let timeline = match &pane.timeline {
        Some(Ok(timeline)) => Some(timeline),
        _ => None,
    };
    let lines: Vec<Line<'static>> = pane
        .rows()
        .iter()
        .enumerate()
        .skip(pane.offset)
        .take(usize::from(inner.height))
        .map(|(at, row)| row_line(app, timeline, row, width, selected == Some(at)))
        .collect();
    frame.render_widget(Paragraph::new(lines), inner);
}

/// A line of the listing, `width` cells wide.
fn row_line(
    app: &App,
    timeline: Option<&Timeline>,
    row: &ChangeRow<'_>,
    width: usize,
    selected: bool,
) -> Line<'static> {
    let line = match row {
        ChangeRow::Heading(text) => return Line::from(format!(" {text}")).bold(),
        ChangeRow::Note(text) => return Line::from(format!(" {}", one_line(text))).dim(),
        ChangeRow::Error(text) => return Line::from(format!(" {}", one_line(text))).red(),
        ChangeRow::Commit(commit) => {
            return Line::from(vec![
                Span::raw("   "),
                Span::styled(one_line(&commit.id), Style::default().fg(Color::Yellow)),
                Span::raw(" "),
                Span::raw(shorten(
                    &commit.subject,
                    width.saturating_sub(commit.id.chars().count() + 4),
                )),
            ]);
        }
        ChangeRow::File(_, file) => file_line(file, width),
        ChangeRow::Entry(entry) => entry_line(app, timeline, entry, width),
    };
    if selected {
        line.style(look::selected_style(true))
    } else {
        line
    }
}

/// `   #7  Claude Code · turn 2      5m ago  +40 −2`: its number, what it
/// was, when it ended, and what it changed.
fn entry_line(
    app: &App,
    timeline: Option<&Timeline>,
    entry: &Entry,
    width: usize,
) -> Line<'static> {
    let number = format!("#{}", entry.number);
    let when = app
        .now
        .map(|now| {
            let ago = muster_core::time::between(entry.ended, now);
            format!("{} ago", look::short_span(ago, false))
        })
        .unwrap_or_default();
    let mut tail = vec![Span::from(when).dim(), Span::raw("  ")];
    tail.extend(match entry.summary {
        Some(changed) => summary(&changed),
        None => vec![Span::from("no files").dim()],
    });
    let tail_width: usize = tail.iter().map(Span::width).sum();
    let lead = 3 + number.chars().count() + 2;
    let room = width.saturating_sub(lead + tail_width + 2);
    let what = shorten(&crate::app::timeline::label(timeline, entry), room.max(4));
    let gap = width
        .saturating_sub(lead + what.chars().count() + tail_width + 1)
        .max(1);
    let mut spans = vec![
        Span::raw("   "),
        Span::from(number).dim(),
        Span::raw("  "),
        Span::raw(what),
        Span::raw(" ".repeat(gap)),
    ];
    spans.extend(tail);
    Line::from(spans)
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

fn draw_diff(frame: &mut Frame<'_>, pane: &ChangesPane, diff: &DiffPane, area: Rect) {
    let title = match &diff.of {
        Pick::File(part, path) => {
            let part = match part {
                ChangePart::Uncommitted => "not committed",
                ChangePart::Branch => "on the branch",
            };
            format!(" {} · {part} ", one_line(path))
        }
        Pick::Entry(number) => {
            let timeline = match &pane.timeline {
                Some(Ok(timeline)) => Some(timeline),
                _ => None,
            };
            let what = timeline
                .and_then(|timeline| {
                    let entry = timeline
                        .entries
                        .iter()
                        .find(|entry| entry.number == *number)?;
                    Some(crate::app::timeline::label(Some(timeline), entry))
                })
                .unwrap_or_default();
            format!(" #{number} · {} ", one_line(&what))
        }
    };
    let block = panel(title, true);
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

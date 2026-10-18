//! Where a workspace's work meets other work in its project (`PRODUCT.md`
//! §6): a word on its line in the list, a line for each other workspace
//! in its details and its changes pane. Paths come from the repository,
//! so they are drawn on one line, cleaned.

use muster_core::clashes::Clash;
use muster_core::text::one_line;
use muster_core::{WorkspaceId, WorkspaceKind};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};

use super::shorten;
use crate::app::App;

/// The most files named on one line; the rest are counted.
const NAMED: usize = 3;

/// The word at the end of a worktree's line, if its work meets other
/// work: `conflict` in red where merging both would conflict, else
/// `overlap` in yellow.
pub(super) fn mark(clashes: &[Clash]) -> Option<Span<'static>> {
    if clashes.iter().any(Clash::would_conflict) {
        Some(Span::styled("conflict", Style::default().fg(Color::Red)))
    } else if clashes.is_empty() {
        None
    } else {
        Some(Span::styled("overlap", Style::default().fg(Color::Yellow)))
    }
}

/// The other workspace as its line in the list names it: `⎇ feature`.
fn other(app: &App, workspace: WorkspaceId) -> String {
    let Some(found) = app.model.workspace(workspace) else {
        return "another worktree".to_owned();
    };
    let symbol = match found.kind {
        WorkspaceKind::Checkout { .. } => "⌂",
        WorkspaceKind::Container { .. } => "⧉",
        WorkspaceKind::Worktree { .. } => "⎇",
    };
    format!("{symbol} {}", one_line(found.kind.branch()))
}

/// What both change, said: where it would conflict if anywhere, else
/// what both change, a few files named and the rest counted.
fn what(clash: &Clash) -> String {
    let (said, count) = if clash.would_conflict() {
        ("would conflict in", clash.conflicts)
    } else {
        ("changes them too:", clash.files)
    };
    let named: Vec<String> = clash
        .shown
        .iter()
        .filter(|file| file.conflict || !clash.would_conflict())
        .take(NAMED)
        .map(|file| one_line(&file.path))
        .collect();
    let more = count.saturating_sub(u64::try_from(named.len()).unwrap_or(u64::MAX));
    let more = if more > 0 {
        format!(" and {more} more")
    } else {
        String::new()
    };
    format!("{said} {}{more}", named.join(", "))
}

fn style(clash: &Clash) -> Style {
    if clash.would_conflict() {
        Style::default().fg(Color::Red)
    } else {
        Style::default().fg(Color::Yellow)
    }
}

/// A line of the changes pane, `width` cells wide: `   ⎇ feature  would
/// conflict in README.md`.
pub(super) fn pane_line(app: &App, clash: &Clash, width: usize) -> Line<'static> {
    let other = other(app, clash.with);
    let room = width.saturating_sub(3 + other.chars().count() + 2);
    Line::from(vec![
        Span::raw("   "),
        Span::raw(other),
        Span::raw("  "),
        Span::styled(shorten(&what(clash), room.max(4)), style(clash)),
    ])
}

/// The lines of a worktree's details, one for each other workspace:
/// `Clashes ⎇ feature would conflict in README.md`.
pub(super) fn detail_lines(app: &App, clashes: &[Clash]) -> Vec<Line<'static>> {
    clashes
        .iter()
        .enumerate()
        .map(|(at, clash)| {
            let label = if at == 0 { "Clashes " } else { "        " };
            Line::from(vec![
                Span::from(label).dim(),
                Span::raw(format!("{} ", other(app, clash.with))),
                Span::styled(what(clash), style(clash)),
            ])
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use muster_core::clashes::ClashFile;

    use super::*;

    fn clash(files: &[(&str, bool)], more: u64) -> Clash {
        let conflicts = files.iter().filter(|(_, conflict)| *conflict).count();
        Clash {
            with: WorkspaceId::new(),
            files: u64::try_from(files.len()).unwrap() + more,
            conflicts: u64::try_from(conflicts).unwrap(),
            shown: files
                .iter()
                .map(|(path, conflict)| ClashFile {
                    path: (*path).to_owned(),
                    conflict: *conflict,
                })
                .collect(),
        }
    }

    #[test]
    fn a_conflict_names_where_and_an_overlap_what() {
        let conflict = clash(&[("src/a.rs", true), ("README.md", false)], 0);
        assert_eq!(what(&conflict), "would conflict in src/a.rs");
        let overlap = clash(&[("a", false), ("b", false), ("c", false), ("d", false)], 2);
        assert_eq!(what(&overlap), "changes them too: a, b, c and 3 more");
        // A path is drawn on one line.
        let odd = clash(&[("two\nlines", true)], 0);
        assert!(!what(&odd).contains('\n'));
    }

    #[test]
    fn the_word_on_the_line_says_conflict_over_overlap() {
        let conflict = clash(&[("a", true)], 0);
        let overlap = clash(&[("b", false)], 0);
        assert_eq!(mark(&[]), None);
        assert_eq!(
            mark(std::slice::from_ref(&overlap)).map(|span| span.content.into_owned()),
            Some("overlap".to_owned())
        );
        assert_eq!(
            mark(&[overlap, conflict]).map(|span| span.content.into_owned()),
            Some("conflict".to_owned())
        );
    }
}

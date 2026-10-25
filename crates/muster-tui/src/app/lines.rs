//! How many lines each list's rows take, and so which row a line shows
//! and how far a list scrolls. A chat's line takes one or two
//! (`Preferences::session_lines`), and a shell's as many, drawn as a chat's;
//! a plugin's panel's, a project's, a worktree's, a ticket's and a
//! setting's one.

use muster_core::settings::SessionLines;
use ratatui::layout::Rect;

use super::App;
use crate::model::Row;

impl App {
    /// How many lines a session's line in the lists takes.
    pub(crate) fn session_height(&self) -> usize {
        match self.model.preferences().session_lines {
            SessionLines::Two => 2,
            SessionLines::One => 1,
        }
    }

    /// How many lines each of the Sessions list's `rows` takes.
    pub(crate) fn row_heights(&self, rows: &[Row]) -> Vec<usize> {
        rows.iter()
            .map(|row| match row {
                // A plugin's panel takes one line, whatever a chat takes; a shell as many.
                Row::Session(id)
                    if self.model.session(*id).is_some_and(|session| {
                        crate::model::stands_beside(session)
                            && session.harness != muster_core::harness::SHELL
                    }) =>
                {
                    1
                }
                Row::Session(_) => self.session_height(),
                Row::Project(_) | Row::Worktree(_) | Row::Terminals => 1,
            })
            .collect()
    }

    /// How many lines each of the Needs you list's lines takes.
    pub(crate) fn needs_heights(&self) -> Vec<usize> {
        vec![self.session_height(); self.needs.list.len()]
    }
}

/// The first row to show of a list whose rows take `heights` lines,
/// `height` lines at a time, moved from `offset` only as far as needed to
/// show all of `selected`, and never leaving empty lines below the last.
pub(super) fn in_view(
    offset: usize,
    selected: Option<usize>,
    heights: &[usize],
    height: usize,
) -> usize {
    // The furthest the list goes and still fills the view.
    let mut furthest = heights.len();
    let mut filled = 0;
    while furthest > 0 && filled + heights[furthest - 1] <= height {
        filled += heights[furthest - 1];
        furthest -= 1;
    }
    let mut offset = offset.min(furthest);
    if let Some(selected) = selected.filter(|selected| *selected < heights.len()) {
        if selected < offset {
            offset = selected;
        }
        while offset < selected && heights[offset..=selected].iter().sum::<usize>() > height {
            offset += 1;
        }
    }
    offset
}

/// Which row is drawn at line `row` of the `list`, a line in from its
/// top and bottom, whose rows take `heights` lines from `offset`: none
/// below the last, or where a row with no room for all its lines is left
/// out, as the list leaves it.
pub(super) fn line_at(list: Rect, row: u16, heights: &[usize], offset: usize) -> Option<usize> {
    let top = list.y + 1;
    let height = usize::from(list.height.saturating_sub(2));
    if row < top || usize::from(row - top) >= height {
        return None;
    }
    let mut line = usize::from(row - top);
    let mut used = 0;
    for (index, lines) in heights.iter().enumerate().skip(offset) {
        used += lines;
        if used > height {
            return None;
        }
        if line < *lines {
            return Some(index);
        }
        line -= lines;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_of_a_line_scroll_as_they_always_did() {
        let ones = [1; 10];
        assert_eq!(in_view(0, Some(3), &ones, 5), 0);
        assert_eq!(in_view(0, Some(7), &ones, 5), 3);
        assert_eq!(in_view(6, Some(2), &ones, 5), 2);
        // Never empty lines below the last.
        assert_eq!(in_view(9, None, &ones, 5), 5);
        assert_eq!(in_view(4, None, &[1; 3], 5), 0);
    }

    #[test]
    fn a_selected_row_of_two_lines_is_shown_whole() {
        // A project, a worktree, then four sessions of two lines each.
        let heights = [1, 1, 2, 2, 2, 2];
        // Lines 0–4 show the project, the worktree, the first session and
        // half the second: the second is selected, so the list moves on.
        assert_eq!(in_view(0, Some(3), &heights, 5), 1);
        assert_eq!(in_view(0, Some(5), &heights, 5), 4);
        assert_eq!(in_view(4, Some(0), &heights, 5), 0);
        // The furthest that still fills five lines starts at the fourth.
        assert_eq!(in_view(5, None, &heights, 5), 4);
    }

    #[test]
    fn a_line_shows_the_row_it_is_part_of() {
        let list = Rect::new(0, 3, 30, 7);
        let heights = [1, 1, 2, 2];
        let at = |row| line_at(list, row, &heights, 0);
        // The label above and the line below hold no row.
        assert_eq!(at(3), None);
        assert_eq!(
            [4, 5, 6, 7, 8].map(at),
            [Some(0), Some(1), Some(2), Some(2), None]
        );
        // The second session has no room for both its lines: it is left
        // out, and the line where it would start holds nothing.
        assert_eq!(at(9), None);
        // Scrolled on by one.
        assert_eq!(line_at(list, 4, &heights, 1), Some(1));
        assert_eq!(line_at(list, 8, &heights, 1), Some(3));
    }
}

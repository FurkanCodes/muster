//! The scroll bars on frames' edges (`PRODUCT.md` §6, `view::scrollbar`):
//! which show (a list, the help, a diff, a terminal scrolled back, each
//! only while what is in it is longer than it shows), how far what each
//! measures goes, and what dragging one moves: a list's selection, as the
//! wheel does there, the help's and a diff's first line, a terminal's
//! view back into its scrollback.

use muster_core::SessionId;
use ratatui::layout::Rect;

use super::{App, Mode, View};
use crate::view::scrollbar;

/// A bar, by what it scrolls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Bar {
    /// The list of the screen shown.
    List,
    /// The help over the screen.
    Help,
    /// A file's or a turn's diff in the changes.
    Diff,
    /// A session's terminal, scrolled back.
    Terminal(SessionId),
}

/// How far what a bar measures goes: its lines, how many show, and the
/// first shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Extent {
    pub(crate) total: usize,
    pub(crate) shown: usize,
    pub(crate) first: usize,
}

impl App {
    /// Every bar on the screen, its track and what it measures; none where
    /// all of it shows.
    pub(crate) fn bars(&self) -> Vec<(Bar, Rect, Extent)> {
        let window = self.window();
        let mut bars = Vec::new();
        if let Mode::Help(help) = &self.mode {
            let area = crate::view::help_area(window);
            let extent = Extent {
                total: help.lines(self.view, &self.keymap).len(),
                shown: usize::from(area.height.saturating_sub(5)),
                first: help.offset,
            };
            bars.push((Bar::Help, scrollbar::track(area), extent));
            return keep_long(bars);
        }
        if matches!(self.mode, Mode::Dialog(_) | Mode::Menu(_)) || self.fills_window() {
            return Vec::new();
        }
        let areas = crate::view::browse_areas(self, window);
        if !areas.list.is_empty() {
            let (heights, first) = self.list_lines();
            let extent = Extent {
                total: heights.iter().sum(),
                shown: usize::from(areas.list.height.saturating_sub(2)),
                first: heights[..first.min(heights.len())].iter().sum(),
            };
            bars.push((Bar::List, scrollbar::track(areas.list), extent));
        }
        if !areas.changes.is_empty()
            && let Some(diff) = self.changes.as_ref().and_then(|pane| pane.diff.as_ref())
            && let Some(Ok(found)) = &diff.text
        {
            let extent = Extent {
                total: found.text.lines().count(),
                shown: usize::from(areas.changes.height.saturating_sub(2)),
                first: diff.scroll,
            };
            bars.push((Bar::Diff, scrollbar::track(areas.changes), extent));
        }
        for (panel, area) in areas.shown() {
            let Some(session) = self.session_in(panel) else {
                continue;
            };
            let Some(screen) = self.screens.get(&session) else {
                continue;
            };
            let back = screen.scrolled_back();
            // At the newest output it shows none, but while dragged there.
            if back == 0 && self.dragging_bar() != Some(Bar::Terminal(session)) {
                continue;
            }
            let rows = usize::from(area.height.saturating_sub(2));
            let extent = Extent {
                total: screen.history() + rows,
                shown: rows,
                first: screen.history().saturating_sub(back),
            };
            bars.push((Bar::Terminal(session), scrollbar::track(area), extent));
        }
        keep_long(bars)
    }

    /// The bar whose track is under `at`, if any.
    pub(crate) fn bar_at(&self, at: ratatui::layout::Position) -> Option<(Bar, Rect)> {
        self.bars()
            .into_iter()
            .find(|(_, track, _)| track.contains(at))
            .map(|(bar, track, _)| (bar, track))
    }

    /// The bar being dragged, if one is.
    pub(crate) fn dragging_bar(&self) -> Option<Bar> {
        match &self.panels.drag {
            Some(super::drag::Drag::Bar { bar, .. }) => Some(*bar),
            _ => None,
        }
    }

    /// The lines each line of the list shown takes, and the first shown.
    fn list_lines(&self) -> (Vec<usize>, usize) {
        match self.view {
            View::Sessions => (self.row_heights(&self.rows()), self.list_offset),
            View::NeedsYou => (self.needs_heights(), self.needs.offset),
            View::Tickets => (vec![1; self.ticket_rows().len()], self.tickets.offset),
            View::Settings => (vec![1; self.setting_rows().len()], self.settings.offset),
        }
    }

    /// Puts `bar`'s thumb at `row` of its track.
    pub(super) fn scroll_bar_to(&mut self, bar: Bar, row: u16) {
        let Some((_, track, extent)) = self.bars().into_iter().find(|(found, _, _)| *found == bar)
        else {
            return;
        };
        let first = scrollbar::first_at(track, extent, row);
        match bar {
            Bar::List => self.select_line(first),
            Bar::Help => {
                if let Mode::Help(help) = &mut self.mode {
                    help.offset = first;
                }
            }
            Bar::Diff => {
                if let Some(diff) = self.changes.as_mut().and_then(|pane| pane.diff.as_mut()) {
                    diff.scroll = first;
                }
            }
            Bar::Terminal(session) => {
                let screen = self.screen(session);
                let back = screen.history().saturating_sub(first);
                let now = screen.scrolled_back();
                screen
                    .scroll(isize::try_from(back).unwrap_or(0) - isize::try_from(now).unwrap_or(0));
            }
        }
    }

    /// Selects the line of the list as far down it as the thumb is put at
    /// `first` (the first line at the top, the last at the foot), as the
    /// wheel moves the selection; the list then scrolls to show it.
    fn select_line(&mut self, first: usize) {
        let (heights, _) = self.list_lines();
        let total: usize = heights.iter().sum();
        let size = usize::from(
            crate::view::browse_areas(self, self.window())
                .list
                .height
                .saturating_sub(2),
        );
        let past = total.saturating_sub(size).max(1);
        let wanted = first.min(past) * total.saturating_sub(1) / past;
        let mut line = 0;
        let mut index = heights.len().saturating_sub(1);
        for (at, height) in heights.iter().enumerate() {
            if line + height > wanted {
                index = at;
                break;
            }
            line += height;
        }
        match self.view {
            View::Sessions => self.selected = self.rows().get(index).copied(),
            View::NeedsYou => self.needs.selected = self.needs.list.get(index).copied(),
            View::Tickets => self.tickets.selected = self.ticket_rows().get(index).cloned(),
            View::Settings => self.settings.selected = self.setting_rows().get(index).copied(),
        }
        self.keep_in_view();
    }
}

/// The bars of `bars` whose content is longer than shows.
fn keep_long(bars: Vec<(Bar, Rect, Extent)>) -> Vec<(Bar, Rect, Extent)> {
    bars.into_iter()
        .filter(|(_, track, extent)| extent.total > extent.shown && track.height > 1)
        .collect()
}

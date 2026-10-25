//! The projects in the order the person likes (`PRODUCT.md` §6): a
//! project's line dragged up or down the sessions list and let go on
//! another project's lines puts it there, the order kept in their own
//! settings (`[layout] projects`, `PreferenceChange::ProjectOrder`), so
//! every window and the next list them alike; the Settings screen's
//! *Projects' order* puts them back as they were added.

use muster_core::ProjectId;
use muster_core::protocol::Command;
use muster_core::settings::PreferenceChange;
use ratatui::layout::{Position, Rect};

use super::lines::line_at;
use super::{Action, App, Then, View};
use crate::model::Row;

impl App {
    /// The project whose line on the sessions list is at `at`, which a
    /// drag up or down moves.
    pub(super) fn project_line_at(&self, at: Position) -> Option<ProjectId> {
        let row = self.row_at(at)?;
        match row {
            Row::Project(project) => Some(project),
            _ => None,
        }
    }

    /// The project whose lines (its own, its worktrees' and its sessions')
    /// `at` is on, where a project dragged there lands.
    pub(super) fn project_under(&self, at: Position) -> Option<ProjectId> {
        let row = self.row_at(at)?;
        self.model.project_of(row)
    }

    /// The sessions list's line at `at`, inside its frame.
    fn row_at(&self, at: Position) -> Option<Row> {
        if self.view != View::Sessions {
            return None;
        }
        let list = crate::view::browse_areas(self, self.window()).list;
        if !list.inner(ratatui::layout::Margin::new(1, 1)).contains(at) {
            return None;
        }
        let rows = self.rows();
        let heights = self.row_heights(&rows);
        rows.get(line_at(list, at.y, &heights, self.list_offset)?)
            .copied()
    }

    /// Where `project`'s own line is drawn on the sessions list, if it is.
    pub(crate) fn project_line_area(&self, project: ProjectId) -> Option<Rect> {
        let list = crate::view::browse_areas(self, self.window()).list;
        let rows = self.rows();
        let heights = self.row_heights(&rows);
        let index = rows.iter().position(|row| *row == Row::Project(project))?;
        let above: usize = heights[self.list_offset.min(index)..index].iter().sum();
        let y = list.y + 1 + u16::try_from(above).ok()?;
        (index >= self.list_offset && y + 1 < list.bottom())
            .then(|| Rect::new(list.x + 1, y, list.width.saturating_sub(2), 1))
    }

    /// `moving` put where `onto` is: after it moving down, before it up.
    pub(super) fn move_project(&mut self, moving: ProjectId, onto: ProjectId) -> Vec<Action> {
        let order: Vec<ProjectId> = self.model.projects().iter().map(|p| p.id).collect();
        let Some(order) = moved(&order, moving, onto) else {
            return Vec::new();
        };
        vec![self.command(
            Command::ChangePreference(PreferenceChange::ProjectOrder(order)),
            Then::Say("The projects are in your order."),
        )]
    }
}

/// `order` with `moving` where `onto` is, after it moving down and before
/// it moving up; `None` where either is not in it, or they are one.
fn moved(order: &[ProjectId], moving: ProjectId, onto: ProjectId) -> Option<Vec<ProjectId>> {
    let from = order.iter().position(|id| *id == moving)?;
    let to = order.iter().position(|id| *id == onto)?;
    if from == to {
        return None;
    }
    let mut order = order.to_vec();
    order.remove(from);
    let at = order.iter().position(|id| *id == onto)? + usize::from(from < to);
    order.insert(at, moving);
    Some(order)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_project_moved_down_goes_after_the_one_it_lands_on_and_up_before() {
        let [a, b, c] = [ProjectId::new(), ProjectId::new(), ProjectId::new()];
        let order = [a, b, c];
        assert_eq!(moved(&order, a, c), Some(vec![b, c, a]));
        assert_eq!(moved(&order, c, a), Some(vec![c, a, b]));
        assert_eq!(moved(&order, b, c), Some(vec![a, c, b]));
        assert_eq!(moved(&order, a, a), None);
        assert_eq!(moved(&order, a, ProjectId::new()), None);
    }
}

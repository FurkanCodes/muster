//! What the pointer could take hold of to move the panels (`PRODUCT.md`
//! §6 Sessions): the line between two panels, or the cell before it, and
//! a panel's title. What is under the pointer is lit as it passes over,
//! so the person sees what a drag would move before they press.

use muster_core::layout::{Branch, Panel};
use ratatui::layout::Position;

use super::App;
use super::drag::Drag;
use crate::layout::Divider;

/// What a press would take hold of.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Hold {
    /// The line between two panels: `on_it` where pressed on the line
    /// itself, not a cell beside it.
    Line { divider: Divider, on_it: bool },
    /// A panel's title.
    Title(Panel),
}

/// What is lit under the pointer: a line, by where it is in the layout, or
/// a panel's title.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Grab {
    Line(Vec<Branch>),
    Title(Panel),
}

impl App {
    /// What a press at `at` would take hold of: the line itself first, then
    /// a title, which keeps its whole row, then a cell beside a line.
    pub(super) fn hold_at(&self, at: Position) -> Option<Hold> {
        let areas = crate::view::browse_areas(self, self.window());
        if let Some(divider) = areas.dividers.iter().find(|line| line.holds(at.x, at.y)) {
            return Some(Hold::Line {
                divider: divider.clone(),
                on_it: true,
            });
        }
        if let Some((panel, _)) = self
            .shown_panels()
            .into_iter()
            .find(|(_, area)| at.y == area.y && (area.x..area.right()).contains(&at.x))
        {
            return Some(Hold::Title(panel));
        }
        areas
            .dividers
            .iter()
            .find(|line| line.near(at.x, at.y))
            .map(|divider| Hold::Line {
                divider: divider.clone(),
                on_it: false,
            })
    }

    /// What to light with the pointer at `at`: the line being dragged, or
    /// what a press there would take hold of; nothing while the panels
    /// cannot be moved, or one is being moved, its landing lit instead.
    pub(super) fn grab_under(&self, at: Position) -> Option<Grab> {
        if self.welcome || !self.can_arrange() {
            return None;
        }
        match &self.drag {
            Some(Drag::Divider { divider, .. }) => Some(Grab::Line(divider.path.clone())),
            Some(Drag::Panel { .. }) => None,
            None => self.hold_at(at).map(|hold| match hold {
                Hold::Line { divider, .. } => Grab::Line(divider.path),
                Hold::Title(panel) => Grab::Title(panel),
            }),
        }
    }
}

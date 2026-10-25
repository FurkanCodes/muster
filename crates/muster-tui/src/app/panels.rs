//! The panels: which are shown, where the person's layout puts them, and
//! the list folded away (`PRODUCT.md` §6 Sessions). The layout and the
//! fold are Muster's own settings, kept by the daemon (`[layout] panels`,
//! `list_folded`); a change the person makes shows at once, and from a
//! daemon before protocol 36, which keeps neither, it lasts as long as the
//! window. A layout the window is too narrow for folds the list first,
//! then shows only the panel with the keys.

use muster_core::layout::{Layout, Panel};
use muster_core::protocol::Command;
use muster_core::settings::PreferenceChange;
use ratatui::layout::Rect;

use super::{Action, App, Dialog, Help, Mode, Then};
use crate::layout::{Solved, needs, solve};

impl App {
    /// Where the panels are: as changed in this window, or as kept.
    pub(crate) fn panels(&self) -> &Layout {
        self.panels
            .kept
            .as_ref()
            .unwrap_or(&self.model.preferences().panels)
    }

    /// Whether the list is folded away.
    pub(crate) fn list_folded(&self) -> bool {
        self.panels
            .folded_kept
            .unwrap_or(self.model.preferences().list_folded)
    }

    /// Whether a workspace's changes are shown: while they, or what is open
    /// over them, have the keys.
    pub(crate) fn changes_shown(&self) -> bool {
        matches!(
            self.mode,
            Mode::Changes
                | Mode::Dialog(Dialog::Undo { .. })
                | Mode::Help(Help {
                    over_changes: true,
                    ..
                })
        )
    }

    /// The panel with the keys, which a window too narrow for the rest
    /// shows alone.
    pub(crate) fn panel_with_keys(&self) -> Panel {
        if self.changes_shown() {
            Panel::Changes
        } else if self.typing_in_plugin() {
            Panel::Plugin
        } else if let Some(placed) = self.typing_in_placed() {
            Panel::Placed(placed)
        } else {
            Panel::Session
        }
    }

    /// The panels laid out over the window's `body`, as many as fit:
    /// every panel shown; then, where there are more than the list and the
    /// session, all but the list; then the list and the panel with the
    /// keys, which opens over the rest. The list and one panel always sit
    /// side by side here, however narrow: the window is as wide as the
    /// person said a session sits beside the list from.
    pub(crate) fn solve_panels(&self, body: Rect, list: u16) -> Solved {
        let with_list = !self.list_folded();
        let keys = self.panel_with_keys();
        let placed = self.placed_beside();
        let showing = |panel: Panel| match panel {
            Panel::List => with_list,
            // Nothing to draw there while the session selected is placed.
            Panel::Session => !self.session_panel_empty(),
            Panel::Changes => self.changes_shown(),
            Panel::Plugin => self.plugin_beside().is_some(),
            Panel::Placed(session) => placed.contains(&session),
        };
        let layout = self.panels();
        let fits = |shown: &dyn Fn(Panel) -> bool| {
            layout
                .shown(shown)
                .filter(|node| needs(node, list).0 <= body.width)
        };
        let more = self.changes_shown() || self.plugin_beside().is_some() || !placed.is_empty();
        let tree = fits(&showing)
            .or_else(|| {
                more.then(|| fits(&|panel| panel != Panel::List && showing(panel)))
                    .flatten()
            })
            .or_else(|| layout.shown(|panel| panel == keys || (panel == Panel::List && with_list)));
        let mut solved = match tree {
            Some(tree) => solve(&tree, body, list),
            None => Solved {
                panels: vec![(keys, body)],
                ..Solved::default()
            },
        };
        // What the panel with the keys shows over, its title says.
        let wanted = layout
            .shown(showing)
            .map_or(0, |node| count(&node, Panel::List));
        let drawn = solved
            .panels
            .iter()
            .filter(|(panel, _)| *panel != Panel::List)
            .count();
        solved.hidden = wanted.saturating_sub(drawn);
        solved
    }

    /// The fold key: folds the list away, so the session takes its place,
    /// or brings it back. It works as the person types in the session too.
    pub(super) fn toggle_list(&mut self) -> Vec<Action> {
        let folded = !self.list_folded();
        self.panels.folded_kept = Some(folded);
        let mut actions = vec![self.command(
            Command::ChangePreference(PreferenceChange::ListFolded(folded)),
            Then::ListFolded,
        )];
        actions.extend(self.fit_attached());
        actions
    }

    /// ⏎ on the Settings screen's *Panels*: every panel back where it was
    /// at first, and the list beside them.
    pub(super) fn put_panels_back(&mut self) -> Vec<Action> {
        if *self.panels() == Layout::default() && !self.list_folded() {
            self.say("The panels are where they were at first.");
            return Vec::new();
        }
        let mut actions = Vec::new();
        if self.list_folded() {
            actions.extend(self.toggle_list());
        }
        actions.extend(self.keep_panels(Layout::default()));
        self.say("The panels are back where they were at first.");
        actions
    }

    /// The daemon kept the fold, which its settings now say, or could not.
    pub(super) fn list_folded_kept(
        &mut self,
        outcome: Result<(), muster_core::protocol::ErrorBody>,
    ) {
        self.panels.folded_kept = None;
        if let Err(error) = outcome {
            self.complain(error.message);
        }
    }
}

/// How many panels `node` holds but `but`.
fn count(node: &muster_core::layout::Node, but: Panel) -> usize {
    match &node.part {
        muster_core::layout::Part::Panel(panel) => usize::from(*panel != but),
        muster_core::layout::Part::Split(_, first, second) => {
            count(first, but) + count(second, but)
        }
    }
}

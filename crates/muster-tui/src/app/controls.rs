//! What a click presses that is neither a panel nor a line (`ARC-32`,
//! `PRODUCT.md` §6): the hints in the bottom bar, each pressing the key it
//! names; the top bar's tabs, counts and ZOOM; the buttons in a panel's
//! title; and the list's fold button, on its frame, or at the window's
//! edge while it is folded. One list of them, each with its cells, which
//! the view lights under the pointer and the mouse presses.

use crossterm::event::KeyEvent;
use muster_core::layout::Panel;
use ratatui::layout::{Position, Rect};

use super::{Action, App, Mode};
use crate::view::pane::Button;
use crate::view::top_bar::TopControl;

/// Something a click presses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Control {
    /// A hint: the key it names.
    Hint(KeyEvent),
    /// A tab, a count or ZOOM in the top bar.
    Top(TopControl),
    /// A button in a panel's title.
    Button(Panel, Button),
    /// The list's fold button: folds it away, or brings it back.
    Fold,
}

impl App {
    /// Everything on the screen a click presses, each where it is; none
    /// under a dialog, which takes its own clicks, nor in a session
    /// filling the window.
    pub(crate) fn controls(&self) -> Vec<(Control, Rect)> {
        if matches!(self.mode, Mode::Dialog(_)) || self.fills_window() || self.welcome {
            return Vec::new();
        }
        let areas = crate::view::browse_areas(self, self.window());
        let mut controls: Vec<(Control, Rect)> =
            crate::view::top_bar::top_controls(self, areas.top)
                .into_iter()
                .map(|(control, cells)| (Control::Top(control), cells))
                .collect();
        for (panel, area) in areas.shown() {
            controls.extend(
                crate::view::pane::button_areas(self, panel, area)
                    .into_iter()
                    .map(|(button, cells)| (Control::Button(panel, button), cells)),
            );
        }
        controls.extend(crate::view::fold_button(self, &areas).map(|cells| (Control::Fold, cells)));
        controls.extend(
            crate::view::bottom_bar::hint_areas(self, areas.bottom)
                .into_iter()
                .map(|(key, cells)| (Control::Hint(key), cells)),
        );
        controls
    }

    /// What a click at `at` presses, if anything.
    pub(crate) fn control_at(&self, at: Position) -> Option<Control> {
        self.controls()
            .into_iter()
            .find(|(_, cells)| cells.contains(at))
            .map(|(control, _)| control)
    }

    /// Where the control under the pointer is, lit: what the runtime draws
    /// again for as the pointer moves.
    pub(crate) fn lit_control(&self) -> Option<Rect> {
        let at = self.person.pointer?;
        self.controls()
            .into_iter()
            .find(|(_, cells)| cells.contains(at))
            .map(|(_, cells)| cells)
    }

    /// Presses `control`.
    pub(super) fn press_control(&mut self, control: Control) -> Vec<Action> {
        match control {
            Control::Hint(key) => self.on_key(key),
            Control::Top(TopControl::Tab(view)) => {
                let mut actions = match self.mode {
                    Mode::Terminal(session) => self.leave_terminal(session),
                    _ => Vec::new(),
                };
                if matches!(self.mode, Mode::Menu(_) | Mode::Help(_)) {
                    self.mode = Mode::Browse;
                }
                actions.extend(self.show(view));
                actions
            }
            Control::Top(TopControl::Zoom) => {
                self.panels.zoomed = None;
                Vec::new()
            }
            Control::Button(panel, button) => self.press_button(panel, button),
            Control::Fold => self.toggle_list(),
        }
    }
}

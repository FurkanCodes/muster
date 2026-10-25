//! The panel key (`PRODUCT.md` §6 Sessions): pressed on the list or while
//! typing in any panel (Ctrl-B unless the person sets another), it waits
//! for one more key, which acts on the panel with the keys (the list's
//! next one while the list has them): split it with a new shell, move the
//! keys to the panel that way, swap with it, zoom it, take it off the
//! screen or close its session, place the preview's session, resize it,
//! go to a panel by its number, give the keys back to the list, show these
//! keys, or send the panel key itself on to the program. Esc, or any key
//! that does none of these, gives up. The arrows move the keys and, with
//! Shift, swap, whatever the person set. A right-click on a panel's title
//! offers the same, each with its keys.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use muster_core::SessionId;
use muster_core::keys::{Action as Does, Screen};
use muster_core::layout::{Edge, Layout, Panel, Place, Split};
use muster_core::protocol::ClientFrame;
use ratatui::layout::Rect;

use super::{Action, App, Here, Mode};
use crate::keys::encode_key;

/// A panel being resized with the keys, and the layout before, which Esc
/// puts back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Resizing {
    pub(crate) panel: Panel,
    was: Layout,
}

/// How many cells a capital moves an edge in resizing.
const LEAP: u16 = 5;

impl App {
    /// The screen whose keys a key pressed now is, for the panel key: the
    /// list's, a session's terminal or the changes; none under a dialog,
    /// a menu or the help.
    fn screen_now(&self) -> Option<Screen> {
        match self.mode {
            Mode::Browse => Some(self.key_screen()),
            Mode::Terminal(_) => Some(Screen::InSession),
            Mode::Changes => Some(Screen::Changes),
            Mode::Dialog(_) | Mode::Menu(_) | Mode::Help(_) => None,
        }
    }

    /// The panel key, the key after it, or a key while resizing, if `key`
    /// is one: what it does, else `None` for the mode to have it.
    pub(super) fn panel_keys(&mut self, key: &KeyEvent) -> Option<Vec<Action>> {
        if let Some(pressed) = self.panels.waiting.take() {
            return Some(self.after_panel_key(pressed, key));
        }
        if let Some(resizing) = self.panels.resizing.clone() {
            return Some(self.resize_key(resizing, key));
        }
        let screen = self.screen_now()?;
        (self.does(screen, key) == Some(Does::PanelKey)).then(|| {
            self.note = None;
            self.panels.waiting = Some(*key);
            Vec::new()
        })
    }

    /// The panel the panel key's next key acts on: the one with the keys,
    /// or the list while it has them.
    pub(crate) fn pane_target(&self) -> Panel {
        match self.mode {
            Mode::Terminal(_) | Mode::Changes => self.panel_with_keys(),
            _ => Panel::List,
        }
    }

    /// The key after the panel key (`pressed`).
    fn after_panel_key(&mut self, pressed: KeyEvent, key: &KeyEvent) -> Vec<Action> {
        // The panel key again: the program has it.
        if crate::keys::chord(key).is_some()
            && crate::keys::chord(key) == crate::keys::chord(&pressed)
        {
            return self.send_on(pressed);
        }
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        let does = match key.code {
            KeyCode::Esc => return Vec::new(),
            KeyCode::Left => Some(if shift {
                Does::SwapLeft
            } else {
                Does::FocusLeft
            }),
            KeyCode::Right => Some(if shift {
                Does::SwapRight
            } else {
                Does::FocusRight
            }),
            KeyCode::Up => Some(if shift { Does::SwapUp } else { Does::FocusUp }),
            KeyCode::Down => Some(if shift {
                Does::SwapDown
            } else {
                Does::FocusDown
            }),
            _ => self.does(Screen::Prefix, key),
        };
        match (does, key.code) {
            (Some(Does::ToPanel), KeyCode::Char(digit)) => {
                let number = digit.to_digit(10).and_then(|n| usize::try_from(n).ok());
                self.to_panel(number.unwrap_or(0))
            }
            (Some(Does::SendPanelKey), _) => self.send_on(pressed),
            (Some(does), _) => self.panel_action(does, self.pane_target()),
            (None, _) => Vec::new(),
        }
    }

    /// Does `does`, one of the keys after the panel key, to `target`: as
    /// the key would, or as the item in its title's menu.
    pub(super) fn panel_action(&mut self, does: Does, target: Panel) -> Vec<Action> {
        let toward = |does: Does| match does {
            Does::FocusLeft | Does::SwapLeft => Edge::Left,
            Does::FocusRight | Does::SwapRight => Edge::Right,
            Does::FocusUp | Does::SwapUp => Edge::Top,
            _ => Edge::Bottom,
        };
        match does {
            Does::ToList => self.keys_to(Panel::List),
            Does::PanelKeys => {
                self.open_help();
                Vec::new()
            }
            _ if !self.can_arrange() => {
                self.complain("Panels are moved in a window wide enough for the list beside them.");
                Vec::new()
            }
            Does::SplitRight => self.split(target, Edge::Right),
            Does::SplitDown => self.split(target, Edge::Bottom),
            Does::FocusLeft | Does::FocusRight | Does::FocusUp | Does::FocusDown => {
                self.focus_toward(target, toward(does))
            }
            Does::SwapLeft | Does::SwapRight | Does::SwapUp | Does::SwapDown => {
                self.swap_toward(target, toward(does))
            }
            Does::Zoom => {
                self.toggle_zoom(target);
                Vec::new()
            }
            Does::TakeOff => self.take_off_panel(target),
            Does::ClosePanel => match self.session_in(target) {
                Some(session) => self.close_session(session),
                None => {
                    self.complain("That panel has no session of its own to close.");
                    Vec::new()
                }
            },
            Does::PlacePreview => match self.preview_session() {
                Some(session) => self.place_at(session, Place::Beside(Panel::Session, Edge::Left)),
                None => {
                    self.complain("The preview shows no session to place.");
                    Vec::new()
                }
            },
            Does::Resize => {
                self.panels.resizing = Some(Resizing {
                    panel: target,
                    was: self.panels().clone(),
                });
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    /// The session `panel` shows, if it shows one.
    pub(crate) fn session_in(&self, panel: Panel) -> Option<SessionId> {
        match panel {
            Panel::Placed(session) => Some(session),
            Panel::Session => self.preview_session(),
            Panel::Plugin => self.plugin_beside(),
            Panel::List | Panel::Changes => None,
        }
    }

    /// A new shell beside `target`, at its `edge`, in its workspace: the
    /// preview's while the list has the keys.
    fn split(&mut self, target: Panel, edge: Edge) -> Vec<Action> {
        let target = if target == Panel::List {
            Panel::Session
        } else {
            target
        };
        let workspace = match target {
            Panel::Changes => self.changes.as_ref().map(|pane| pane.workspace),
            panel => self
                .session_in(panel)
                .and_then(|session| self.model.session(session))
                .map(|stored| stored.workspace),
        }
        .or_else(|| self.selected_workspace());
        let Some(workspace) = workspace else {
            self.complain("Select a session, a worktree or a project to open a shell in it.");
            return Vec::new();
        };
        self.start_shell(workspace, Place::Beside(target, edge))
    }

    /// The panels numbered while the panel key waits: those shown that
    /// hold a terminal, in reading order.
    pub(crate) fn numbered_panels(&self) -> Vec<(Panel, Rect)> {
        let mut shown: Vec<(Panel, Rect)> = self
            .shown_panels()
            .into_iter()
            .filter(|(panel, _)| matches!(panel, Panel::Session | Panel::Placed(_) | Panel::Plugin))
            .collect();
        shown.sort_by_key(|(_, area)| (area.y, area.x));
        shown
    }

    /// The keys to the panel numbered `number`.
    fn to_panel(&mut self, number: usize) -> Vec<Action> {
        match self.numbered_panels().get(number.wrapping_sub(1)) {
            Some((panel, _)) => self.keys_to(*panel),
            None => Vec::new(),
        }
    }

    /// The keys to `panel`: the list's, a terminal's or the changes'.
    pub(super) fn keys_to(&mut self, panel: Panel) -> Vec<Action> {
        if let Some(zoomed) = self.panels.zoomed
            && zoomed != panel
            && panel != Panel::List
        {
            self.panels.zoomed = Some(panel);
        }
        match panel {
            Panel::List => match self.mode {
                Mode::Terminal(session) => self.leave_terminal(session),
                Mode::Changes => {
                    self.close_changes();
                    Vec::new()
                }
                _ => Vec::new(),
            },
            Panel::Changes => {
                if self.changes.is_some() {
                    self.mode = Mode::Changes;
                }
                Vec::new()
            }
            panel => {
                if let Some(session) = self.session_in(panel) {
                    self.full_screen = false;
                    self.mode = Mode::Terminal(session);
                }
                Vec::new()
            }
        }
    }

    /// The keys to the panel past `target`'s `edge`; where the window
    /// shows one over the rest, the next or the one before, round them.
    fn focus_toward(&mut self, target: Panel, edge: Edge) -> Vec<Action> {
        if let Some(next) = self.round_hidden(target, edge) {
            return self.keys_to(next);
        }
        let shown = self.shown_panels();
        let Some(area) = shown
            .iter()
            .find(|(panel, _)| *panel == target)
            .map(|(_, area)| *area)
        else {
            return Vec::new();
        };
        match super::drag::neighbour(&shown, area, edge) {
            Some(next) => self.keys_to(next),
            None => Vec::new(),
        }
    }

    /// Where the panels are shown one over the rest (zoomed, or a window
    /// too narrow for them), the panel after `target` that way round them
    /// in the layout's order, the list left out.
    fn round_hidden(&self, target: Panel, edge: Edge) -> Option<Panel> {
        let areas = crate::view::browse_areas(self, self.window());
        if areas.hidden == 0 {
            return None;
        }
        let wanted: Vec<Panel> = self
            .wanted_panels()
            .into_iter()
            .filter(|panel| *panel != Panel::List)
            .collect();
        let at = wanted.iter().position(|panel| *panel == target)?;
        let back = matches!(edge, Edge::Left | Edge::Top);
        let next = if back {
            (at + wanted.len() - 1) % wanted.len()
        } else {
            (at + 1) % wanted.len()
        };
        wanted.get(next).copied()
    }

    /// `target` and the panel past its `edge` in each other's places.
    fn swap_toward(&mut self, target: Panel, edge: Edge) -> Vec<Action> {
        let shown = self.shown_panels();
        let Some(area) = shown
            .iter()
            .find(|(panel, _)| *panel == target)
            .map(|(_, area)| *area)
        else {
            return Vec::new();
        };
        match super::drag::neighbour(&shown, area, edge) {
            Some(other) => {
                let swapped = self.panels().swapped(target, other);
                self.keep_panels(swapped)
            }
            None => Vec::new(),
        }
    }

    /// Zooms `target` over the others beside the list, or puts it back.
    fn toggle_zoom(&mut self, target: Panel) {
        if self.panels.zoomed.is_some() {
            self.panels.zoomed = None;
        } else if target == Panel::List {
            self.complain("The list is not zoomed; fold it away to give the panels its room.");
        } else {
            self.panels.zoomed = Some(target);
        }
    }

    /// Takes `target` off the screen: a placed terminal running on, the
    /// preview folded away, the changes closed, the list folded.
    fn take_off_panel(&mut self, target: Panel) -> Vec<Action> {
        match target {
            Panel::Placed(session) => self.take_off(session),
            Panel::Session => {
                self.fold_preview();
                Vec::new()
            }
            Panel::Changes => {
                self.close_changes();
                Vec::new()
            }
            Panel::List => self.toggle_list(),
            Panel::Plugin => {
                self.complain("A plugin's panel goes away on its own key, or its line's menu.");
                Vec::new()
            }
        }
    }

    /// Folds the preview away until the list selects another session.
    pub(super) fn fold_preview(&mut self) {
        if let Some(session) = self.preview_session() {
            if self.mode == Mode::Terminal(session) {
                self.mode = Mode::Browse;
            }
            self.panels.preview_folded = Some(session);
            self.say("The preview is folded away until you select another session.");
        }
    }

    /// The panel key, sent on to the program with the keys.
    fn send_on(&mut self, pressed: KeyEvent) -> Vec<Action> {
        let Mode::Terminal(session) = self.mode else {
            return Vec::new();
        };
        let modes = self.screen(session).modes;
        encode_key(&pressed, modes)
            .map(|data| Action::Send(ClientFrame::Input { session, data }))
            .into_iter()
            .collect()
    }

    /// A key while a panel is resized: h j k l (or the arrows) move its
    /// edge that way a cell, H J K L five (`nudge`); ⏎ keeps it, Esc puts
    /// it back.
    fn resize_key(&mut self, resizing: Resizing, key: &KeyEvent) -> Vec<Action> {
        let cells = |c: char| if c.is_ascii_uppercase() { LEAP } else { 1 };
        let (edge, by) = match key.code {
            KeyCode::Char(c @ ('h' | 'H')) => (Edge::Left, cells(c)),
            KeyCode::Char(c @ ('l' | 'L')) => (Edge::Right, cells(c)),
            KeyCode::Char(c @ ('k' | 'K')) => (Edge::Top, cells(c)),
            KeyCode::Char(c @ ('j' | 'J')) => (Edge::Bottom, cells(c)),
            KeyCode::Left => (Edge::Left, 1),
            KeyCode::Right => (Edge::Right, 1),
            KeyCode::Up => (Edge::Top, 1),
            KeyCode::Down => (Edge::Bottom, 1),
            KeyCode::Enter => {
                self.panels.resizing = None;
                let now = self.panels().clone();
                return if now == resizing.was {
                    Vec::new()
                } else {
                    self.keep_panels(now)
                };
            }
            KeyCode::Esc => {
                self.panels.resizing = None;
                self.panels.kept = Some(resizing.was);
                return Vec::new();
            }
            _ => return Vec::new(),
        };
        self.nudge(resizing.panel, edge, by);
        Vec::new()
    }

    /// Moves `panel`'s edge `by` cells toward `edge`: its right edge (its
    /// bottom one, up or down), else, at the window's edge, its left (its
    /// top), so h undoes l and a panel at the window's edge still resizes.
    fn nudge(&mut self, panel: Panel, edge: Edge, by: u16) {
        let areas = crate::view::browse_areas(self, self.window());
        let area = areas.of(panel);
        if area.is_empty() {
            return;
        }
        let (split, forward) = match edge {
            Edge::Left => (Split::Across, false),
            Edge::Right => (Split::Across, true),
            Edge::Top => (Split::Down, false),
            Edge::Bottom => (Split::Down, true),
        };
        let (start, end) = match split {
            Split::Across => (area.x, area.right()),
            Split::Down => (area.y, area.bottom()),
        };
        let line = areas
            .dividers
            .iter()
            .filter(|divider| divider.split == split)
            .find(|divider| divider.cut == end)
            .or_else(|| {
                areas
                    .dividers
                    .iter()
                    .filter(|divider| divider.split == split)
                    .find(|divider| divider.cut == start)
            });
        let Some(line) = line else {
            return;
        };
        let first = if forward {
            line.first_extent().saturating_add(by)
        } else {
            line.first_extent().saturating_sub(by).max(1)
        };
        let resized = self.resized_at(line, first);
        self.panels.kept = Some(resized);
    }

    /// What a right-click on `panel`'s title offers, each with its keys:
    /// the panel key's.
    pub(super) fn title_items(&self, panel: Panel) -> Vec<Here> {
        let shown = self.shown_panels();
        let area = shown
            .iter()
            .find(|(shown, _)| *shown == panel)
            .map(|(_, area)| *area);
        let beside = |edge| area.and_then(|area| super::drag::neighbour(&shown, area, edge));
        let mut items = vec![
            self.after_panel(Does::SplitRight, "split right, a shell"),
            self.after_panel(Does::SplitDown, "split down, a shell"),
        ];
        if panel != Panel::List {
            let zoom = if self.panels.zoomed.is_some() {
                "put back"
            } else {
                "zoom"
            };
            items.push(self.after_panel(Does::Zoom, zoom));
        }
        for (edge, does, what) in [
            (Edge::Left, Does::SwapLeft, "swap left"),
            (Edge::Right, Does::SwapRight, "swap right"),
            (Edge::Top, Does::SwapUp, "swap up"),
            (Edge::Bottom, Does::SwapDown, "swap down"),
        ] {
            if beside(edge).is_some() {
                items.push(self.after_panel(does, what));
            }
        }
        items.push(self.after_panel(Does::Resize, "resize"));
        if panel == Panel::Session && self.preview_session().is_some() {
            items.push(self.after_panel(Does::PlacePreview, "place it here"));
        }
        if matches!(panel, Panel::Placed(_) | Panel::Session) {
            items.push(self.after_panel(Does::TakeOff, "off the screen"));
        }
        if self.session_in(panel).is_some() {
            items.push(self.after_panel(Does::ClosePanel, "close the session"));
        }
        items
    }

    /// `does`, one of the keys after the panel key, as a menu's item: its
    /// key after the panel key's.
    fn after_panel(&self, does: Does, what: &str) -> Here {
        Here {
            does: Some(does),
            panel: None,
            key: format!(
                "{} {}",
                self.keymap.label(Does::PanelKey),
                self.keymap.label(does)
            ),
            what: what.to_owned(),
        }
    }
}

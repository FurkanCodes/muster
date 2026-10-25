//! Moving the panels (`PRODUCT.md` §6 Sessions). With the mouse: a panel
//! taken by its title goes where it is dropped, at an edge of the window
//! or of another panel, the place lighting up as it would land; the line
//! between two panels, dragged, shares out their place, the programs
//! given their new sizes as it is let go. With the keys: `m` picks the
//! panel with the keys, ⇥ another, the arrows move it past its neighbour,
//! `+` and `-` make it wider or narrower, ⏎ keeps the layout and Esc puts
//! it back. A press on a title without moving gives that panel the keys,
//! as a click on the panel does. A line is taken by the cell before it
//! too; from there it moves once the pointer has gone two cells, and a
//! press that does not is the click it was.
//!
//! A panel dropped on another's middle swaps places with it. A session's
//! line in the list, dragged out of it, places the session in a panel of
//! its own where it is dropped (`app::placed`); the press on it selects it
//! as any does. A press on a button in a title (`view::pane`) does what it
//! says rather than take the panel.

use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use muster_core::layout::{Branch, Edge, Layout, Panel, Place, Size, Split};
use muster_core::protocol::Command;
use muster_core::settings::PreferenceChange;
use ratatui::layout::{Position, Rect};

use super::grab::Hold;
use super::{Action, App, Mode, Then, View};
use crate::layout::Divider;

/// The fewest and the most columns the person can drag the list to.
const LIST_COLUMNS: (u16, u16) = (20, 80);

/// The least share of a place the person can give either of two panels.
const LEAST_SHARE: u8 = 10;

/// A drag under way.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Drag {
    /// A panel taken by its title: whether it has moved, and where it would
    /// land now, if anywhere.
    Panel {
        panel: Panel,
        moved: bool,
        to: Option<Place>,
    },
    /// A session's line taken from the list: the press that took it, and
    /// where its panel would land now, once it has left the list.
    Line {
        session: muster_core::SessionId,
        press: MouseEvent,
        to: Option<Place>,
    },
    /// The line between two panels, and the layout before it moved; the
    /// press that took it, and whether it has moved; `beside` where it was
    /// taken by a cell beside it, not on it.
    Divider {
        divider: Divider,
        was: Layout,
        press: MouseEvent,
        beside: bool,
        moved: bool,
    },
}

/// The panels being moved with the keys: the one picked, and the layout
/// before, which Esc puts back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Arranging {
    pub(crate) panel: Panel,
    was: Layout,
}

impl App {
    pub(super) fn window(&self) -> Rect {
        Rect::new(0, 0, self.terminal_size.cols, self.terminal_size.rows + 1)
    }

    /// Whether the panels can be moved now: beside the list, rather than in
    /// a window too narrow for it or full screen, and not under a dialog, a
    /// menu or the help.
    pub(super) fn can_arrange(&self) -> bool {
        !self.narrow()
            && !self.fills_window()
            && matches!(self.mode, Mode::Browse | Mode::Terminal(_) | Mode::Changes)
    }

    /// The panels shown, each where it is drawn.
    pub(super) fn shown_panels(&self) -> Vec<(Panel, Rect)> {
        crate::view::browse_areas(self, self.window()).shown()
    }

    /// The mouse, for a drag: a press on a panel's title or on a line
    /// between two (or beside it) starts one, and while one is under way
    /// it takes every event. `None` leaves the event to the mode.
    pub(super) fn drag_mouse(&mut self, event: MouseEvent) -> Option<Vec<Action>> {
        let at = Position::new(event.column, event.row);
        if event.kind == MouseEventKind::Down(MouseButton::Left) && self.panels.drag.is_some() {
            // A press while a drag is under way: its release never came, so
            // it is let go, and this press is one of its own.
            self.cancel_drag();
        }
        if let Some(drag) = self.panels.drag.clone() {
            return Some(match event.kind {
                MouseEventKind::Drag(MouseButton::Left) => {
                    self.drag_to(drag, at);
                    Vec::new()
                }
                MouseEventKind::Up(MouseButton::Left) => self.drop_at(drag, at),
                _ => Vec::new(),
            });
        }
        if event.kind != MouseEventKind::Down(MouseButton::Left) || !self.can_arrange() {
            return None;
        }
        if let Some((panel, button)) = crate::view::pane::button_at(self, &self.shown_panels(), at)
        {
            return Some(self.press_button(panel, button));
        }
        if let Some(session) = self.session_line_at(at) {
            // The press is the click it always was, and may become a drag.
            let actions = self.mode_mouse(event);
            self.panels.drag = Some(Drag::Line {
                session,
                press: event,
                to: None,
            });
            return Some(actions);
        }
        // What is marked in a session is clicked away, as by any click.
        self.panels.drag = Some(match self.hold_at(at)? {
            Hold::Line { divider, on_it } => Drag::Divider {
                divider,
                was: self.panels().clone(),
                press: event,
                beside: !on_it,
                moved: false,
            },
            Hold::Title(panel) => Drag::Panel {
                panel,
                moved: false,
                to: None,
            },
        });
        self.selection = None;
        Some(Vec::new())
    }

    fn drag_to(&mut self, drag: Drag, at: Position) {
        match drag {
            Drag::Line { session, press, .. } => {
                let list = crate::view::browse_areas(self, self.window()).list;
                let to = (!list.contains(at))
                    .then(|| self.drop_place(Panel::Placed(session), at, false))
                    .flatten();
                self.panels.drag = Some(Drag::Line { session, press, to });
            }
            Drag::Panel { panel, .. } => {
                self.panels.drag = Some(Drag::Panel {
                    panel,
                    moved: true,
                    to: self.drop_place(panel, at, true),
                });
            }
            Drag::Divider {
                divider,
                was,
                press,
                beside,
                moved,
            } => {
                // Moved as far as the pointer, from where it was taken.
                let (from, to) = match divider.split {
                    Split::Across => (press.column, at.x),
                    Split::Down => (press.row, at.y),
                };
                if beside && !moved && from.abs_diff(to) < 2 {
                    return;
                }
                let first = (i32::from(divider.first_extent()) + i32::from(to) - i32::from(from))
                    .clamp(1, i32::from(u16::MAX));
                let first = u16::try_from(first).unwrap_or(1);
                let resized = self.resized_at(&divider, first);
                self.panels.kept = Some(resized);
                self.panels.drag = Some(Drag::Divider {
                    divider,
                    was,
                    press,
                    beside,
                    moved: true,
                });
            }
        }
    }

    fn drop_at(&mut self, drag: Drag, at: Position) -> Vec<Action> {
        match drag {
            Drag::Line {
                session,
                to: Some(place),
                ..
            } => {
                self.panels.drag = None;
                self.place_at(session, place)
            }
            Drag::Line { press, .. } => {
                // Let go in the list: the click it was, already taken.
                self.panels.drag = None;
                self.mode_mouse(MouseEvent {
                    kind: MouseEventKind::Up(MouseButton::Left),
                    column: at.x,
                    row: at.y,
                    ..press
                })
            }
            Drag::Panel { panel, moved, to } => {
                self.panels.drag = None;
                match (moved, to) {
                    (false, _) => self.give_keys(panel),
                    (true, Some(place)) => {
                        let moved = self.panels().moved(panel, place);
                        self.keep_panels(moved)
                    }
                    (true, None) => Vec::new(),
                }
            }
            Drag::Divider {
                press,
                beside: true,
                moved: false,
                ..
            } if press.column.abs_diff(at.x) < 2 && press.row.abs_diff(at.y) < 2 => {
                // Pressed beside the line and not moved: the click it was.
                self.panels.drag = None;
                let mut actions = self.mode_mouse(press);
                actions.extend(self.mode_mouse(MouseEvent {
                    kind: MouseEventKind::Up(MouseButton::Left),
                    ..press
                }));
                actions
            }
            Drag::Divider { ref was, .. } => {
                let was = was.clone();
                self.drag_to(drag, at);
                self.panels.drag = None;
                let now = self.panels().clone();
                if now == was {
                    return Vec::new();
                }
                self.keep_panels(now)
            }
        }
    }

    /// Lets go of a drag under way, the layout as it was.
    pub(super) fn cancel_drag(&mut self) {
        if let Some(Drag::Divider { was, .. }) = self.panels.drag.take() {
            self.panels.kept = Some(was);
        }
    }

    /// A press on `panel`'s title, not moved: the panel gets the keys.
    fn give_keys(&mut self, panel: Panel) -> Vec<Action> {
        match (panel, &self.mode) {
            (Panel::List, Mode::Terminal(session)) => {
                let session = *session;
                self.leave_terminal(session)
            }
            (Panel::List, Mode::Changes) => {
                self.close_changes();
                Vec::new()
            }
            (Panel::Session, Mode::Browse | Mode::Changes)
                if matches!(self.view, View::Sessions | View::NeedsYou)
                    && self.focused_session().is_some() =>
            {
                self.close_changes();
                self.open_beside_list();
                Vec::new()
            }
            (Panel::Placed(session), _) => {
                self.close_changes();
                self.mode = Mode::Terminal(session);
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    /// Where `panel` dropped at `at` would land: along an edge of the
    /// window near it, or at the nearest edge of the panel under it, or,
    /// where it can `swap`, in that panel's place, about its middle. Over
    /// itself, nowhere.
    fn drop_place(&self, panel: Panel, at: Position, swap: bool) -> Option<Place> {
        let areas = crate::view::browse_areas(self, self.window());
        let body = body(&areas);
        if at.x <= body.x {
            return Some(Place::Window(Edge::Left));
        }
        if at.x + 1 >= body.right() {
            return Some(Place::Window(Edge::Right));
        }
        if at.y < body.y {
            return Some(Place::Window(Edge::Top));
        }
        if at.y + 1 >= body.bottom() {
            return Some(Place::Window(Edge::Bottom));
        }
        let (target, area) = self
            .shown_panels()
            .into_iter()
            .find(|(_, area)| area.contains(at))?;
        if target == panel {
            return None;
        }
        // Rows are about twice as tall as columns are wide.
        let edges = [
            (at.x - area.x, Edge::Left),
            (area.right() - 1 - at.x, Edge::Right),
            ((at.y - area.y) * 2, Edge::Top),
            ((area.bottom() - 1 - at.y) * 2, Edge::Bottom),
        ];
        let (nearest, edge) = edges.into_iter().min_by_key(|(distance, _)| *distance)?;
        // The middle: as far from every edge as a quarter of the panel.
        let middle = (area.width / 4).min(area.height / 2);
        if swap && nearest > middle.max(1) {
            return Some(Place::Over(target));
        }
        Some(Place::Beside(target, edge))
    }

    /// A press on `button` in `panel`'s title.
    fn press_button(&mut self, panel: Panel, button: crate::view::pane::Button) -> Vec<Action> {
        use crate::view::pane::Button;
        match (button, panel) {
            (Button::TakeOff, Panel::Placed(session)) => self.take_off(session),
            (Button::TakeOff, Panel::Session) => {
                if let Some(session) = self.preview_session() {
                    if self.mode == Mode::Terminal(session) {
                        self.mode = Mode::Browse;
                    }
                    self.panels.preview_folded = Some(session);
                    self.say("The preview is folded away until you select another session.");
                }
                Vec::new()
            }
            (Button::Place, Panel::Session) => match self.preview_session() {
                Some(session) => self.place_at(session, Place::Beside(Panel::Session, Edge::Left)),
                None => Vec::new(),
            },
            _ => Vec::new(),
        }
    }

    /// Where a panel put at `place` would land, as it lights up.
    pub(crate) fn landing(&self, place: Place) -> Rect {
        let areas = crate::view::browse_areas(self, self.window());
        let area_of = |panel: Panel| {
            self.shown_panels()
                .into_iter()
                .find(|(shown, _)| *shown == panel)
                .map_or(Rect::default(), |(_, area)| area)
        };
        let (area, edge) = match place {
            Place::Window(edge) => (body(&areas), edge),
            Place::Beside(panel, edge) => (area_of(panel), edge),
            // Swapped in, it takes the whole of the other's place.
            Place::Over(panel) => return area_of(panel),
        };
        let (width, height) = match place {
            Place::Window(_) => ((area.width / 4).max(10), (area.height / 3).max(4)),
            Place::Beside(..) | Place::Over(_) => (area.width / 2, area.height / 2),
        };
        match edge {
            Edge::Left => Rect { width, ..area },
            Edge::Right => Rect {
                x: area.right() - width,
                width,
                ..area
            },
            Edge::Top => Rect { height, ..area },
            Edge::Bottom => Rect {
                y: area.bottom() - height,
                height,
                ..area
            },
        }
    }

    /// The layout with `divider` moved so the first of its two takes
    /// `first` cells: the list a width of its own, others a share.
    fn resized_at(&self, divider: &Divider, first: u16) -> Layout {
        let layout = self.panels().clone();
        let Some(path) = layout.split_between(&divider.first, &divider.second) else {
            return layout;
        };
        let total = match divider.split {
            Split::Across => divider.area.width,
            Split::Down => divider.area.height,
        };
        let first = first.clamp(1, total.saturating_sub(1).max(1));
        let list_alone = |side: &[Panel]| divider.split == Split::Across && side == [Panel::List];
        let (branch, size) = if list_alone(&divider.first) {
            (
                Branch::First,
                Size::Cells(first.clamp(LIST_COLUMNS.0, LIST_COLUMNS.1)),
            )
        } else if list_alone(&divider.second) {
            let list = total - first;
            (
                Branch::Second,
                Size::Cells(list.clamp(LIST_COLUMNS.0, LIST_COLUMNS.1)),
            )
        } else {
            let share = u32::from(first) * 100 / u32::from(total.max(1));
            let share = u8::try_from(share)
                .unwrap_or(100)
                .clamp(LEAST_SHARE, 100 - LEAST_SHARE);
            (Branch::First, Size::Share(share))
        };
        layout.sized(&path, branch, size)
    }

    /// Shows `layout` and has the daemon keep it.
    pub(super) fn keep_panels(&mut self, layout: Layout) -> Vec<Action> {
        self.panels.kept = Some(layout.clone());
        vec![self.command(
            Command::ChangePreference(PreferenceChange::Panels(layout)),
            Then::PanelsKept,
        )]
    }

    /// The daemon kept the layout, which its settings now say, or could
    /// not. Not while the person is moving the panels again.
    pub(super) fn panels_kept(&mut self, outcome: Result<(), muster_core::protocol::ErrorBody>) {
        if self.panels.drag.is_none() && self.panels.arranging.is_none() {
            self.panels.kept = None;
        }
        if let Err(error) = outcome {
            self.panels.kept = None;
            self.complain(error.message);
        }
    }

    /// `m`: moves the panel with the keys with the keys.
    pub(super) fn start_arranging(&mut self) -> Vec<Action> {
        if !self.can_arrange() {
            self.complain("The panels move in a window wide enough for the list beside them.");
            return Vec::new();
        }
        let panel = match self.mode {
            Mode::Terminal(_) | Mode::Changes => self.panel_with_keys(),
            _ => Panel::List,
        };
        self.panels.arranging = Some(Arranging {
            panel,
            was: self.panels().clone(),
        });
        Vec::new()
    }

    pub(super) fn arrange_key(&mut self, key: &KeyEvent) -> Vec<Action> {
        let Some(arranging) = self.panels.arranging.clone() else {
            return Vec::new();
        };
        let shown = self.shown_panels();
        let Some(&(_, area)) = shown.iter().find(|(panel, _)| *panel == arranging.panel) else {
            self.panels.arranging = None;
            return Vec::new();
        };
        match key.code {
            KeyCode::Tab | KeyCode::BackTab => {
                let at = shown
                    .iter()
                    .position(|(panel, _)| *panel == arranging.panel)
                    .unwrap_or(0);
                let next = if key.code == KeyCode::Tab {
                    (at + 1) % shown.len()
                } else {
                    (at + shown.len() - 1) % shown.len()
                };
                self.panels.arranging = Some(Arranging {
                    panel: shown[next].0,
                    ..arranging
                });
            }
            KeyCode::Left | KeyCode::Right | KeyCode::Up | KeyCode::Down => {
                let edge = match key.code {
                    KeyCode::Left => Edge::Left,
                    KeyCode::Right => Edge::Right,
                    KeyCode::Up => Edge::Top,
                    _ => Edge::Bottom,
                };
                let place = neighbour(&shown, area, edge)
                    .map_or(Place::Window(edge), |next| Place::Beside(next, edge));
                self.panels.kept = Some(self.panels().moved(arranging.panel, place));
            }
            KeyCode::Char(c @ ('+' | '=' | '-')) => {
                let grow = c != '-';
                let areas = crate::view::browse_areas(self, self.window());
                if let Some((divider, first)) = beside(&areas.dividers, area, grow) {
                    self.panels.kept = Some(self.resized_at(divider, first));
                }
            }
            KeyCode::Enter => {
                self.panels.arranging = None;
                let now = self.panels().clone();
                if now != arranging.was {
                    return self.keep_panels(now);
                }
            }
            KeyCode::Esc => {
                self.panels.arranging = None;
                self.panels.kept = Some(arranging.was);
            }
            _ => {}
        }
        Vec::new()
    }
}

/// The window's body: between the top bar and the bottom one.
fn body(areas: &crate::view::BrowseAreas) -> Rect {
    Rect {
        x: areas.top.x,
        y: areas.top.bottom(),
        width: areas.top.width,
        height: areas.bottom.y.saturating_sub(areas.top.bottom()),
    }
}

/// The nearest panel past `area`'s `edge`, overlapping it the other way.
fn neighbour(shown: &[(Panel, Rect)], area: Rect, edge: Edge) -> Option<Panel> {
    let across = |other: Rect| other.y < area.bottom() && area.y < other.bottom();
    let down = |other: Rect| other.x < area.right() && area.x < other.right();
    shown
        .iter()
        .filter_map(|(panel, other)| {
            let gap = match edge {
                Edge::Left if across(*other) && other.right() <= area.x => area.x - other.right(),
                Edge::Right if across(*other) && other.x >= area.right() => other.x - area.right(),
                Edge::Top if down(*other) && other.bottom() <= area.y => area.y - other.bottom(),
                Edge::Bottom if down(*other) && other.y >= area.bottom() => other.y - area.bottom(),
                _ => return None,
            };
            Some((gap, *panel))
        })
        .min_by_key(|(gap, _)| *gap)
        .map(|(_, panel)| panel)
}

/// The line beside `area` side by side that a key moves, its right one
/// first, and the cells the first of its two takes once `area` is two
/// columns wider, or narrower.
fn beside(dividers: &[Divider], area: Rect, grow: bool) -> Option<(&Divider, u16)> {
    let step: i32 = if grow { 2 } else { -2 };
    dividers
        .iter()
        .filter(|divider| divider.split == Split::Across)
        .find_map(|divider| {
            let first = i32::from(divider.cut) - i32::from(divider.area.x);
            let first = if divider.cut == area.right() {
                first + step
            } else if divider.cut == area.x {
                first - step
            } else {
                return None;
            };
            u16::try_from(first.max(1))
                .ok()
                .map(|first| (divider, first))
        })
}

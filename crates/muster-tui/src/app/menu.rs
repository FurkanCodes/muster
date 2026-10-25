//! The menu a right-click on a line of *Sessions* or *Needs you* opens:
//! what can be done there (`app::actions`), each with its key, so the
//! keys can be found without knowing them; and the one a right-click on a
//! panel's title opens, what the panel key does to it (`app::prefix`).
//! Choosing one does what its key does; Esc or a click outside closes it.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::{Action, App, Here, Mode, View};

/// A menu open on the selected line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Menu {
    /// The cell clicked: the menu opens just below it.
    pub(crate) at: (u16, u16),
    pub(crate) items: Vec<Here>,
    /// The item ↑↓ are on, which ⏎ chooses.
    pub(crate) chosen: usize,
    /// The panel whose title it opened on; `None` on a line.
    pub(crate) on: Option<muster_core::layout::Panel>,
}

impl App {
    /// A right-click on the list at `at`: the line there is selected, and
    /// the menu of what can be done on it opens, if anything can.
    pub(super) fn open_menu(&mut self, list: Rect, at: Position) {
        let on_a_line = self.has_line_at(list, at.y);
        self.select_at(list, at.y);
        let items = match self.view {
            // The list's empty part: what starts something there.
            View::Sessions | View::NeedsYou if !on_a_line => self.empty_list_items(),
            View::Sessions | View::NeedsYou => self.actions_here(),
            // A ticket's or a setting's: what its hints say, each its key.
            View::Tickets | View::Settings => crate::view::line_hints(self)
                .iter()
                .filter_map(|hint| Here::of_hint(hint))
                .collect(),
        };
        if items.is_empty() {
            return;
        }
        self.note = None;
        self.mode = Mode::Menu(Menu {
            at: (at.x, at.y),
            items,
            chosen: 0,
            on: None,
        });
    }

    /// A right-click on `panel`'s title at `at`: what the panel key does to
    /// it, as a menu.
    pub(super) fn open_title_menu(&mut self, panel: muster_core::layout::Panel, at: Position) {
        self.note = None;
        // What is marked in a session is clicked away, as by any click.
        self.selection = None;
        self.mode = Mode::Menu(Menu {
            at: (at.x, at.y),
            items: self.title_items(panel),
            chosen: 0,
            on: Some(panel),
        });
    }

    /// A key while the menu is open: ↑↓ choose, ⏎ does the chosen, Esc
    /// closes it; any other key closes it and does what it does on the
    /// list, so an item's own key does that item.
    pub(super) fn menu_key(&mut self, mut menu: Menu, key: &KeyEvent) -> Vec<Action> {
        let last = menu.items.len().saturating_sub(1);
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                menu.chosen = menu.chosen.saturating_sub(1);
                self.mode = Mode::Menu(menu);
                Vec::new()
            }
            KeyCode::Down | KeyCode::Char('j') => {
                menu.chosen = (menu.chosen + 1).min(last);
                self.mode = Mode::Menu(menu);
                Vec::new()
            }
            KeyCode::Enter => match menu.items.get(menu.chosen) {
                Some(item) => self.choose(&menu, item),
                None => self.close_menu(),
            },
            // On a title, an item's own key is the one after the panel key.
            _ if menu.on.is_some() => {
                let does = self.does(muster_core::keys::Screen::Prefix, key);
                match menu
                    .items
                    .iter()
                    .find(|item| item.does.is_some() && item.does == does)
                {
                    Some(item) => self.choose(&menu, &item.clone()),
                    None => self.close_menu(),
                }
            }
            KeyCode::Esc => self.close_menu(),
            _ => {
                self.mode = Mode::Browse;
                self.on_key(*key)
            }
        }
    }

    /// The mouse while the menu is open: a click on an item does it, a
    /// click outside closes the menu (and a right-click there opens one
    /// where it is); the wheel and the rest do nothing.
    pub(super) fn menu_mouse(&mut self, menu: &Menu, event: MouseEvent) -> Vec<Action> {
        let window = Rect::new(0, 0, self.terminal_size.cols, self.terminal_size.rows + 1);
        let area = crate::view::menu_area(menu, window);
        let at = Position::new(event.column, event.row);
        let MouseEventKind::Down(pressed @ (MouseButton::Left | MouseButton::Right)) = event.kind
        else {
            return Vec::new();
        };
        if area.contains(at) {
            let item = usize::from(at.y.saturating_sub(area.y + 1));
            return match menu.items.get(item) {
                Some(item) if pressed == MouseButton::Left && at.y > area.y => {
                    self.choose(menu, item)
                }
                _ => Vec::new(),
            };
        }
        let mut actions = self.close_menu();
        if pressed == MouseButton::Right {
            actions.extend(self.on_mouse(event));
        }
        actions
    }

    /// Whether the list at `list` draws a line at `row`.
    fn has_line_at(&self, list: Rect, row: u16) -> bool {
        use super::lines::line_at;
        match self.view {
            View::Sessions => {
                let rows = self.rows();
                line_at(list, row, &self.row_heights(&rows), self.list_offset).is_some()
            }
            View::NeedsYou => {
                line_at(list, row, &self.needs_heights(), self.needs.offset).is_some()
            }
            View::Tickets => {
                let heights = vec![1; self.ticket_rows().len()];
                line_at(list, row, &heights, self.tickets.offset).is_some()
            }
            View::Settings => {
                let heights = vec![1; self.setting_rows().len()];
                line_at(list, row, &heights, self.settings.offset).is_some()
            }
        }
    }

    /// What a right-click on the empty part of the list offers: a project
    /// to add or clone, the session told of last, and every key.
    pub(super) fn empty_list_items(&self) -> Vec<Here> {
        use muster_core::keys::Action as Does;
        [
            (Does::AddProject, "add a project"),
            (Does::CloneProject, "clone one from GitHub"),
            (Does::NewTerminal, "a terminal in no project"),
            (Does::Spaces, "spaces"),
            (Does::LastAlarm, "the session told of last"),
            (Does::Help, "every key"),
        ]
        .into_iter()
        .map(|(does, what)| Here {
            does: Some(does),
            panel: None,
            key: self.keymap.label(does),
            what: what.to_owned(),
            choice: None,
        })
        .collect()
    }

    /// Does what `item` of `menu` says, as its key would.
    fn choose(&mut self, menu: &Menu, item: &Here) -> Vec<Action> {
        self.mode = Mode::Browse;
        if let Some(choice) = &item.choice {
            return self.choose_space(choice.clone());
        }
        if let (Some(panel), Some(does)) = (menu.on, item.does) {
            // The keys back where they were, for the panel key's next key.
            return self.panel_action(does, panel);
        }
        match (item.does, &item.panel) {
            // What the list's keys do, they do from the menu too.
            (Some(does), _) if !matches!(self.view, View::Sessions | View::NeedsYou) => {
                self.press_does(does)
            }
            (
                Some(
                    does @ (muster_core::keys::Action::AddProject
                    | muster_core::keys::Action::CloneProject
                    | muster_core::keys::Action::NewTerminal
                    | muster_core::keys::Action::Spaces
                    | muster_core::keys::Action::LastAlarm
                    | muster_core::keys::Action::Help),
                ),
                _,
            ) => self.press_does(does),
            (Some(does), _) => self.list_action(does),
            (None, Some(panel)) => self.toggle_panel(panel),
            // ⏎, or the key a hint named.
            (None, None) => {
                let key = crate::keys::labelled(&item.key)
                    .unwrap_or_else(|| KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
                self.on_key(key)
            }
        }
    }

    /// Presses `does`'s first key, as the person would.
    fn press_does(&mut self, does: muster_core::keys::Action) -> Vec<Action> {
        match self.keymap.chords(does).first() {
            Some(chord) => {
                let key = crate::keys::press(*chord);
                self.on_key(key)
            }
            None => Vec::new(),
        }
    }

    fn close_menu(&mut self) -> Vec<Action> {
        self.mode = Mode::Browse;
        Vec::new()
    }
}

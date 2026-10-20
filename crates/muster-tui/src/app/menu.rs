//! The menu a right-click on a line of *Sessions* or *Needs you* opens:
//! what can be done there (`app::actions`), each with its key, so the
//! keys can be found without knowing them. Choosing one does what its key does;
//! Esc or a click outside closes it.

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
}

impl App {
    /// A right-click on the list at `at`: the line there is selected, and
    /// the menu of what can be done on it opens, if anything can.
    pub(super) fn open_menu(&mut self, list: Rect, at: Position) {
        if !matches!(self.view, View::Sessions | View::NeedsYou) {
            return;
        }
        self.select_at(list, at.y);
        let items = self.actions_here();
        if items.is_empty() {
            return;
        }
        self.note = None;
        self.mode = Mode::Menu(Menu {
            at: (at.x, at.y),
            items,
            chosen: 0,
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
                Some(item) => self.choose(item),
                None => self.close_menu(),
            },
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
                Some(item) if pressed == MouseButton::Left && at.y > area.y => self.choose(item),
                _ => Vec::new(),
            };
        }
        let mut actions = self.close_menu();
        if pressed == MouseButton::Right {
            actions.extend(self.on_mouse(event));
        }
        actions
    }

    /// Does what `item` says, as its key would.
    fn choose(&mut self, item: &Here) -> Vec<Action> {
        self.mode = Mode::Browse;
        match (item.does, &item.panel) {
            (Some(does), _) => self.list_action(does),
            (None, Some(panel)) => self.toggle_panel(panel),
            (None, None) => self.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        }
    }

    fn close_menu(&mut self) -> Vec<Action> {
        self.mode = Mode::Browse;
        Vec::new()
    }
}

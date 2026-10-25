//! The mouse in a dialog (`ARC-32`, `PRODUCT.md` §6): a click on a key its
//! hint names presses it; on one of its choices (an agent, a tracker, a
//! project), a field, a folder or a repository offered, or an action in
//! the keys' dialog, presses the keys that choose it, as the person would,
//! so the dialog's own rules decide; a double-click there does what ⏎
//! does too; a click outside closes it as Esc does; and the wheel moves
//! through what it offers as ↑↓ do.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Position;

use super::{Action, App, Dialog, Mode, RebindRow};
use crate::view::dialogs::DialogPress;

/// The most keys a click presses to reach what it chose: past them it was
/// not to be reached.
const MOST_KEYS: usize = 64;

impl App {
    /// The mouse while `dialog` is open.
    pub(super) fn dialog_mouse(&mut self, dialog: &Dialog, event: MouseEvent) -> Vec<Action> {
        let at = Position::new(event.column, event.row);
        let (rect, presses) = crate::view::dialogs::dialog_presses(self, dialog, self.window());
        let key = |code| KeyEvent::new(code, KeyModifiers::NONE);
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) if !rect.contains(at) => {
                self.on_key(key(KeyCode::Esc))
            }
            MouseEventKind::Down(MouseButton::Left) => {
                let double = self.person.double;
                match presses.into_iter().find(|(cells, _)| cells.contains(at)) {
                    Some((_, press)) => {
                        let offered =
                            matches!(press, DialogPress::Offered(_) | DialogPress::Row(_));
                        let mut actions = self.dialog_press(press);
                        if double && offered && matches!(self.mode, Mode::Dialog(_)) {
                            actions.extend(self.on_key(key(KeyCode::Enter)));
                        }
                        actions
                    }
                    None => Vec::new(),
                }
            }
            MouseEventKind::ScrollUp if rect.contains(at) => self.on_key(key(KeyCode::Up)),
            MouseEventKind::ScrollDown if rect.contains(at) => self.on_key(key(KeyCode::Down)),
            _ => Vec::new(),
        }
    }

    /// Does what a click on `press` says.
    fn dialog_press(&mut self, press: DialogPress) -> Vec<Action> {
        match press {
            DialogPress::Key(key) => self.on_key(key),
            DialogPress::Harness(harness) => self.press_until(KeyCode::Tab, move |app| {
                app.dialog_harness().is_some_and(|chosen| *chosen == harness)
            }),
            DialogPress::Tracker(tracker) => self.press_until(KeyCode::Tab, move |app| {
                matches!(&app.mode, Mode::Dialog(Dialog::TicketsFrom { tracker: chosen, .. }) if *chosen == tracker)
            }),
            DialogPress::Project(project) => self.press_until(KeyCode::Down, move |app| {
                matches!(&app.mode, Mode::Dialog(Dialog::AssignAgent { into, .. }) if *into == project)
            }),
            DialogPress::Field(field) => self.step_to(field, |app| match &app.mode {
                Mode::Dialog(Dialog::TicketsFrom { at, .. }) => Some(*at),
                _ => None,
            }),
            DialogPress::Offered(at) => match &self.mode {
                Mode::Dialog(Dialog::NamingModel { listed: Some(Ok(models)), .. }) => {
                    let wanted = models.get(at).cloned().unwrap_or_default();
                    self.press_until(KeyCode::Tab, move |app| {
                        matches!(&app.mode, Mode::Dialog(Dialog::NamingModel { model, .. }) if model.text() == wanted)
                    })
                }
                _ => self.step_to(at, |app| match &app.mode {
                    Mode::Dialog(Dialog::AddProject { picker, .. }) => picker.selected,
                    Mode::Dialog(Dialog::CloneProject(choice)) => choice.selected,
                    _ => None,
                }),
            },
            DialogPress::Row(row) => self.step_to(row, |app| match &app.mode {
                Mode::Dialog(Dialog::Keys(rebinding)) => {
                    let chosen = RebindRow::Key(app.rebinding_chosen(rebinding));
                    app.rebind_rows().iter().position(|row| *row == chosen)
                }
                _ => None,
            }),
        }
    }

    /// The agent the dialog open offers, chosen.
    fn dialog_harness(&self) -> Option<&super::HarnessChoice> {
        match &self.mode {
            Mode::Dialog(
                Dialog::NewSession { harness, .. }
                | Dialog::AssignAgent { harness, .. }
                | Dialog::Tidy { harness, .. },
            ) => Some(harness),
            _ => None,
        }
    }

    /// Presses `code` until `done` says so, as the person would, but no
    /// more than [`MOST_KEYS`] times, nor once the dialog has gone.
    fn press_until(&mut self, code: KeyCode, done: impl Fn(&Self) -> bool) -> Vec<Action> {
        let mut actions = Vec::new();
        for _ in 0..MOST_KEYS {
            if done(self) || !matches!(self.mode, Mode::Dialog(_)) {
                break;
            }
            actions.extend(self.on_key(KeyEvent::new(code, KeyModifiers::NONE)));
        }
        actions
    }

    /// Presses ↓ or ↑ until what `now` says is chosen is `wanted`.
    fn step_to(&mut self, wanted: usize, now: impl Fn(&Self) -> Option<usize>) -> Vec<Action> {
        let mut actions = Vec::new();
        for _ in 0..MOST_KEYS {
            let code = match now(self) {
                Some(at) if at == wanted => break,
                Some(at) if at > wanted => KeyCode::Up,
                _ => KeyCode::Down,
            };
            if !matches!(self.mode, Mode::Dialog(_)) {
                break;
            }
            actions.extend(self.on_key(KeyEvent::new(code, KeyModifiers::NONE)));
        }
        actions
    }
}

//! A one-line text field for dialogs.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct LineInput {
    text: String,
    /// The cursor, as a character index.
    cursor: usize,
}

/// What a key did to a field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Edit {
    Changed,
    Submit,
    Cancel,
    Ignored,
}

impl LineInput {
    pub(crate) fn with(text: &str) -> Self {
        Self {
            text: text.to_owned(),
            cursor: text.chars().count(),
        }
    }

    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    /// The text before the cursor, for drawing the cursor in the right place.
    pub(crate) fn before_cursor(&self) -> &str {
        let end = self
            .text
            .char_indices()
            .nth(self.cursor)
            .map_or(self.text.len(), |(index, _)| index);
        &self.text[..end]
    }

    pub(crate) fn handle(&mut self, key: &KeyEvent) -> Edit {
        let control = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Enter => Edit::Submit,
            KeyCode::Esc => Edit::Cancel,
            KeyCode::Char('u') if control => {
                self.text.clear();
                self.cursor = 0;
                Edit::Changed
            }
            KeyCode::Char(c) if !control => {
                let at = self.byte_at(self.cursor);
                self.text.insert(at, c);
                self.cursor += 1;
                Edit::Changed
            }
            KeyCode::Backspace if self.cursor > 0 => {
                self.cursor -= 1;
                let at = self.byte_at(self.cursor);
                self.text.remove(at);
                Edit::Changed
            }
            KeyCode::Delete if self.cursor < self.text.chars().count() => {
                let at = self.byte_at(self.cursor);
                self.text.remove(at);
                Edit::Changed
            }
            KeyCode::Left if self.cursor > 0 => {
                self.cursor -= 1;
                Edit::Changed
            }
            KeyCode::Right if self.cursor < self.text.chars().count() => {
                self.cursor += 1;
                Edit::Changed
            }
            KeyCode::Home => {
                self.cursor = 0;
                Edit::Changed
            }
            KeyCode::End => {
                self.cursor = self.text.chars().count();
                Edit::Changed
            }
            _ => Edit::Ignored,
        }
    }

    /// Adds pasted text at the cursor, on one line.
    pub(crate) fn paste(&mut self, text: &str) {
        for c in text.chars().filter(|c| !c.is_control()) {
            let at = self.byte_at(self.cursor);
            self.text.insert(at, c);
            self.cursor += 1;
        }
    }

    fn byte_at(&self, cursor: usize) -> usize {
        self.text
            .char_indices()
            .nth(cursor)
            .map_or(self.text.len(), |(index, _)| index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(input: &mut LineInput, code: KeyCode) -> Edit {
        input.handle(&KeyEvent::new(code, KeyModifiers::NONE))
    }

    #[test]
    fn typing_editing_and_moving() {
        let mut input = LineInput::default();
        for c in "fix-lgin".chars() {
            press(&mut input, KeyCode::Char(c));
        }
        for _ in 0..3 {
            press(&mut input, KeyCode::Left);
        }
        press(&mut input, KeyCode::Char('o'));
        assert_eq!(input.text(), "fix-login");
        assert_eq!(input.before_cursor(), "fix-lo");
        press(&mut input, KeyCode::End);
        press(&mut input, KeyCode::Backspace);
        assert_eq!(input.text(), "fix-logi");
        assert_eq!(press(&mut input, KeyCode::Enter), Edit::Submit);
        assert_eq!(press(&mut input, KeyCode::Esc), Edit::Cancel);
    }

    #[test]
    fn wide_characters_and_paste() {
        let mut input = LineInput::with("ü");
        input.paste("ber\nall");
        assert_eq!(input.text(), "überall");
        press(&mut input, KeyCode::Home);
        press(&mut input, KeyCode::Delete);
        assert_eq!(input.text(), "berall");
    }
}

//! How many columns of the terminal text takes, as ratatui lays it out: a
//! wide character (most of Chinese, Japanese and Korean, many emoji) takes
//! two, a combining one none. Anything that measures or cuts text to fit
//! goes through here, never by its count of characters.

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// The columns `text` takes.
pub(crate) fn width(text: &str) -> usize {
    text.width()
}

fn char_width(c: char) -> usize {
    c.width().unwrap_or(0)
}

/// The longest start of `text` that fits in `room` columns.
pub(crate) fn start_within(text: &str, room: usize) -> &str {
    let mut used = 0;
    for (at, c) in text.char_indices() {
        used += char_width(c);
        if used > room {
            return &text[..at];
        }
    }
    text
}

/// The longest end of `text` that fits in `room` columns.
pub(crate) fn end_within(text: &str, room: usize) -> &str {
    let mut used = 0;
    for (at, c) in text.char_indices().rev() {
        used += char_width(c);
        if used > room {
            return &text[at + c.len_utf8()..];
        }
    }
    text
}

/// `text` after its first `columns` columns; a wide character they end in
/// the middle of goes with them.
pub(crate) fn after(text: &str, columns: usize) -> &str {
    let mut used = 0;
    for (at, c) in text.char_indices() {
        if used >= columns {
            return &text[at..];
        }
        used += char_width(c);
    }
    ""
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_characters_take_two_columns_and_are_never_cut_in_half() {
        assert_eq!(width("ab"), 2);
        assert_eq!(width("日本"), 4);
        assert_eq!(width("e\u{301}"), 1, "a combining accent takes none");
        assert_eq!(start_within("日本語", 3), "日");
        assert_eq!(start_within("日本語", 4), "日本");
        assert_eq!(end_within("日本語", 3), "語");
        assert_eq!(after("日本語", 1), "本語");
        assert_eq!(after("日本語", 2), "本語");
        assert_eq!(after("ab", 5), "");
    }
}

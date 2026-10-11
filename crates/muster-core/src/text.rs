//! Text from outside Muster made safe to show: issue titles, labels and
//! bodies (anyone can file an issue, `PRD-07`), directory names, git's
//! messages. A control character in such text could reach a terminal as an
//! escape sequence and make it do things (retitle a window, rewrite the
//! screen, answer queries as keystrokes), so these are taken out.

/// `text` on one line, for titles, names and labels: line breaks and tabs
/// become spaces, every other control character is dropped.
pub fn one_line(text: &str) -> String {
    text.chars()
        .filter_map(|c| match c {
            '\n' | '\r' | '\t' => Some(' '),
            c if c.is_control() => None,
            c => Some(c),
        })
        .collect()
}

/// `text` keeping its line breaks and tabs, for bodies and messages:
/// `\r\n` and lone `\r` become `\n`, every other control character is
/// dropped.
pub fn multi_line(text: &str) -> String {
    text.replace("\r\n", "\n")
        .chars()
        .filter_map(|c| match c {
            '\n' | '\t' => Some(c),
            '\r' => Some('\n'),
            c if c.is_control() => None,
            c => Some(c),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_line_text_loses_breaks_and_escapes() {
        assert_eq!(
            one_line("Fix\tthe\r\nlogin\x1b]0;owned\x07 now\u{9b}2J\u{7f}"),
            "Fix the  login]0;owned now2J"
        );
        assert_eq!(one_line("plain ünïcode 🙂"), "plain ünïcode 🙂");
    }

    #[test]
    fn multi_line_text_keeps_its_lines() {
        assert_eq!(
            multi_line("one\r\ntwo\rthree\n\tfour\x1b[31m\x08"),
            "one\ntwo\nthree\n\tfour[31m"
        );
    }
}

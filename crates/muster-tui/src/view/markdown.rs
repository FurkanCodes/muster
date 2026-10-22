//! A ticket's text and its comments as Markdown, drawn in the terminal: the
//! subset the desktop app draws (headings, paragraphs, lists, quotes,
//! fenced code, rules; bold, italic, code and links inside a line), the
//! rest left as it was written. Anyone can write a ticket (`PRD-07`): its
//! text arrives cleaned of control characters, and nothing here opens or
//! loads anything, so a link is its words and its address, an image its
//! words.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use super::theme::Theme;

/// How a rule (`---`) is drawn: wide enough to part, short enough to fit.
const RULE: &str = "────────────────────────";

/// `text` as lines, in `theme`'s colours.
pub(crate) fn lines(text: &str, theme: &Theme) -> Vec<Line<'static>> {
    let looks = Looks::of(theme);
    let mut out: Vec<Line<'static>> = Vec::new();
    let mut source = text.lines().peekable();
    while let Some(line) = source.next() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(fence) = fence_of(trimmed) {
            part(&mut out);
            for code in source.by_ref() {
                if code.trim_start().starts_with(fence) {
                    break;
                }
                out.push(Line::from(vec![
                    Span::styled("  ", looks.code),
                    Span::styled(code.to_owned(), looks.code),
                ]));
            }
            continue;
        }
        if is_rule(trimmed) {
            part(&mut out);
            out.push(Line::styled(RULE, looks.quiet));
            continue;
        }
        if let Some(heading) = heading_of(trimmed) {
            part(&mut out);
            out.push(Line::from(inline(heading, looks.heading, &looks)));
            continue;
        }
        if trimmed.starts_with('>') {
            part(&mut out);
            let mut quoted = vec![line];
            while let Some(next) = source.next_if(|next| next.trim_start().starts_with('>')) {
                quoted.push(next);
            }
            for said in quoted {
                let said = said.trim_start().trim_start_matches('>');
                let said = said.strip_prefix(' ').unwrap_or(said);
                let mut spans = vec![Span::styled("▎ ", looks.quiet)];
                spans.extend(inline(said, looks.quote, &looks));
                out.push(Line::from(spans));
            }
            continue;
        }
        if item_of(line).is_some() {
            part(&mut out);
            let mut items = vec![line];
            while let Some(next) =
                source.next_if(|next| item_of(next).is_some() || is_continued(next))
            {
                items.push(next);
            }
            for item in items {
                out.push(item_line(item, &looks));
            }
            continue;
        }
        part(&mut out);
        out.push(Line::from(inline(trimmed, Style::default(), &looks)));
        // A paragraph's lines stay as written: the terminal wraps them.
        while let Some(next) = source.next_if(|next| starts_paragraph_line(next)) {
            out.push(Line::from(inline(
                next.trim_start(),
                Style::default(),
                &looks,
            )));
        }
    }
    out
}

/// A blank line between blocks, never two, never first.
fn part(out: &mut Vec<Line<'static>>) {
    if out.last().is_some_and(|line| !line.spans.is_empty()) {
        out.push(Line::default());
    }
}

/// The styles text is drawn in.
struct Looks {
    heading: Style,
    quote: Style,
    quiet: Style,
    code: Style,
    link: Style,
}

impl Looks {
    fn of(theme: &Theme) -> Self {
        Self {
            heading: Style::default().add_modifier(Modifier::BOLD),
            quote: theme.quiet.add_modifier(Modifier::ITALIC),
            quiet: theme.quiet,
            code: Style::default().bg(theme.tint),
            link: Style::default().add_modifier(Modifier::UNDERLINED),
        }
    }
}

/// The fence a code block opens with, if `line` opens one: three
/// backticks or three tildes.
fn fence_of(line: &str) -> Option<&'static str> {
    if line.starts_with("```") {
        Some("```")
    } else if line.starts_with("~~~") {
        Some("~~~")
    } else {
        None
    }
}

/// Whether `line` is a rule: three or more of `-`, `*` or `_`, alone.
fn is_rule(line: &str) -> bool {
    let line: String = line.chars().filter(|c| !c.is_whitespace()).collect();
    line.len() >= 3
        && ["-", "*", "_"]
            .iter()
            .any(|mark| line.chars().all(|c| c.to_string() == *mark))
}

/// A heading's words, if `line` is one: `#` to `######` and a space.
fn heading_of(line: &str) -> Option<&str> {
    let marks = line.chars().take_while(|c| *c == '#').count();
    if !(1..=6).contains(&marks) {
        return None;
    }
    let rest = &line[marks..];
    if rest.is_empty() {
        return Some("");
    }
    rest.strip_prefix(' ')
        .map(|words| words.trim().trim_end_matches('#').trim_end())
}

/// A list item's depth, its mark as drawn and its words, if `line` is
/// one: `- `, `* `, `+ ` or `1. ` (or `1) `), two spaces of indent a level.
fn item_of(line: &str) -> Option<(usize, String, &str)> {
    let indent = line.len() - line.trim_start().len();
    let rest = line.trim_start();
    let depth = indent / 2;
    if let Some(words) = ["- ", "* ", "+ "]
        .iter()
        .find_map(|mark| rest.strip_prefix(mark))
    {
        // A task: its box ticked or not.
        if let Some(words) = words.strip_prefix("[ ] ") {
            return Some((depth, "☐".to_owned(), words));
        }
        if let Some(words) = words
            .strip_prefix("[x] ")
            .or_else(|| words.strip_prefix("[X] "))
        {
            return Some((depth, "☑".to_owned(), words));
        }
        return Some((depth, "•".to_owned(), words));
    }
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    if (1..=9).contains(&digits) {
        let after = &rest[digits..];
        let words = after
            .strip_prefix(". ")
            .or_else(|| after.strip_prefix(") "))?;
        return Some((depth, format!("{}.", &rest[..digits]), words));
    }
    None
}

/// Whether `line` goes on the list item before it: indented, not blank.
fn is_continued(line: &str) -> bool {
    !line.trim().is_empty() && line.starts_with("  ") && item_of(line).is_none()
}

/// A list item, or a line going on one, drawn with its mark and indent.
fn item_line(line: &str, looks: &Looks) -> Line<'static> {
    let (lead, words) = if let Some((depth, mark, words)) = item_of(line) {
        (format!("{}{mark} ", "  ".repeat(depth + 1)), words)
    } else {
        ("    ".to_owned(), line.trim_start())
    };
    let mut spans = vec![Span::raw(lead)];
    spans.extend(inline(words, Style::default(), looks));
    Line::from(spans)
}

/// Whether `line` goes on the paragraph before it: not blank, and opening
/// no block of its own.
fn starts_paragraph_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    !trimmed.is_empty()
        && fence_of(trimmed).is_none()
        && heading_of(trimmed).is_none()
        && !trimmed.starts_with('>')
        && item_of(line).is_none()
        && !is_rule(trimmed)
}

/// The spans of one line of text in `base`: `code`, **bold**, *italic* or
/// _italic_, [words](address) and ![words](address); a mark without its
/// match is left as written.
fn inline(text: &str, base: Style, looks: &Looks) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut plain = String::new();
    let mut rest = text;
    let flush = |plain: &mut String, spans: &mut Vec<Span<'static>>| {
        if !plain.is_empty() {
            spans.push(Span::styled(std::mem::take(plain), base));
        }
    };
    while let Some(c) = rest.chars().next() {
        if c == '`'
            && let Some(end) = rest[1..].find('`')
            && end > 0
        {
            flush(&mut plain, &mut spans);
            spans.push(Span::styled(
                rest[1..=end].to_owned(),
                base.patch(looks.code),
            ));
            rest = &rest[end + 2..];
            continue;
        }
        if let Some((words, address, len)) = link(rest) {
            flush(&mut plain, &mut spans);
            let image = rest.starts_with('!');
            if image {
                spans.push(Span::styled(format!("[image: {words}]"), looks.quiet));
            } else {
                spans.extend(
                    inline(words, base, looks)
                        .into_iter()
                        .map(|span| span.patch_style(looks.link)),
                );
            }
            if !address.is_empty() && address != words {
                spans.push(Span::styled(format!(" ({address})"), looks.quiet));
            }
            rest = &rest[len..];
            continue;
        }
        if let Some((inner, len)) = between(rest, "**").or_else(|| between(rest, "__")) {
            flush(&mut plain, &mut spans);
            spans.extend(inline(inner, base.add_modifier(Modifier::BOLD), looks));
            rest = &rest[len..];
            continue;
        }
        let at_word_start = plain
            .chars()
            .last()
            .is_none_or(|before| !before.is_alphanumeric());
        if let Some((inner, len)) =
            between(rest, "*").or_else(|| at_word_start.then(|| between(rest, "_")).flatten())
        {
            flush(&mut plain, &mut spans);
            spans.extend(inline(inner, base.add_modifier(Modifier::ITALIC), looks));
            rest = &rest[len..];
            continue;
        }
        plain.push(c);
        rest = &rest[c.len_utf8()..];
    }
    flush(&mut plain, &mut spans);
    spans
}

/// The words between `mark` at the start of `text` and its next `mark`,
/// and how long the whole is; none where it does not close, or holds
/// nothing, or starts or ends with a space.
fn between<'a>(text: &'a str, mark: &str) -> Option<(&'a str, usize)> {
    let inner_start = text.strip_prefix(mark)?;
    let end = inner_start.find(mark)?;
    let inner = &inner_start[..end];
    if inner.is_empty() || inner.starts_with(' ') || inner.ends_with(' ') {
        return None;
    }
    Some((inner, mark.len() * 2 + inner.len()))
}

/// A link at the start of `text` (or an image, after `!`): its words, its
/// address and how long the whole is.
fn link(text: &str) -> Option<(&str, &str, usize)> {
    let skip = usize::from(text.starts_with('!'));
    let rest = text[skip..].strip_prefix('[')?;
    let close = rest.find("](")?;
    let words = &rest[..close];
    if words.contains('[') {
        return None;
    }
    let after = &rest[close + 2..];
    let end = after.find(')')?;
    let address = after[..end].trim();
    if address.contains(char::is_whitespace) {
        return None;
    }
    Some((words, address, skip + 1 + close + 2 + end + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme() -> Theme {
        Theme::new(
            muster_core::settings::Palette::Muster,
            &muster_core::colours::TerminalColours::default(),
        )
    }

    fn shown(text: &str) -> Vec<String> {
        lines(text, &theme())
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect()
            })
            .collect()
    }

    fn styled<'a>(lines: &'a [Line<'static>], words: &str) -> &'a Span<'static> {
        lines
            .iter()
            .flat_map(|line| &line.spans)
            .find(|span| span.content == words)
            .unwrap_or_else(|| panic!("no span {words:?}"))
    }

    #[test]
    fn blocks_are_drawn_one_after_another_a_blank_line_between() {
        let text = "# Login fails\n\nAfter signing in,\nusers land on /404.\n\n- one\n- two\n  still two\n\n1. first\n2) second\n\n> it was fine\n> last week\n\n---\n```rust\nfn main() {}\n```";
        assert_eq!(
            shown(text),
            [
                "Login fails",
                "",
                "After signing in,",
                "users land on /404.",
                "",
                "  • one",
                "  • two",
                "    still two",
                "",
                "  1. first",
                "  2. second",
                "",
                "▎ it was fine",
                "▎ last week",
                "",
                RULE,
                "",
                "  fn main() {}",
            ]
        );
    }

    #[test]
    fn inside_a_line_marks_become_looks() {
        let drawn = lines(
            "**bold** and *slanted* and _too_ and `code` and snake_case_name",
            &theme(),
        );
        assert!(
            styled(&drawn, "bold")
                .style
                .add_modifier
                .contains(Modifier::BOLD)
        );
        assert!(
            styled(&drawn, "slanted")
                .style
                .add_modifier
                .contains(Modifier::ITALIC)
        );
        assert!(
            styled(&drawn, "too")
                .style
                .add_modifier
                .contains(Modifier::ITALIC)
        );
        assert_eq!(styled(&drawn, "code").style.bg, Some(theme().tint));
        assert_eq!(
            shown("**bold** and *slanted* and _too_ and `code` and snake_case_name"),
            ["bold and slanted and too and code and snake_case_name"]
        );
    }

    #[test]
    fn a_link_is_its_words_and_its_address_and_an_image_is_never_loaded() {
        assert_eq!(
            shown("See [the docs](https://example.com/docs) and ![a cat](https://x/cat.png)."),
            ["See the docs (https://example.com/docs) and [image: a cat] (https://x/cat.png)."]
        );
        let drawn = lines("[the docs](https://example.com/docs)", &theme());
        assert!(
            styled(&drawn, "the docs")
                .style
                .add_modifier
                .contains(Modifier::UNDERLINED)
        );
        // A link whose words are its address says it once.
        assert_eq!(shown("[https://x.y](https://x.y)"), ["https://x.y"]);
    }

    #[test]
    fn marks_without_their_match_and_tasks_read_as_written() {
        assert_eq!(
            shown("2 * 3 = 6, a ` alone, [not a link]"),
            ["2 * 3 = 6, a ` alone, [not a link]"]
        );
        assert_eq!(shown("- [ ] to do\n- [x] done"), ["  ☐ to do", "  ☑ done"]);
        assert_eq!(shown("  - nested"), ["    • nested"]);
        assert_eq!(shown("#hashtag"), ["#hashtag"]);
    }

    #[test]
    fn a_fence_left_open_keeps_the_rest_as_code() {
        assert_eq!(
            shown("```\nlet a = 1;\n\nlet b = 2;"),
            ["  let a = 1;", "  ", "  let b = 2;"]
        );
    }
}

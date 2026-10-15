//! Text from HTML, for tickets whose tracker sends their text as HTML
//! (Jira, through Atlassian's MCP server). Paragraphs, line breaks, lists,
//! headings, links, tables and code are kept as plain text a terminal can
//! show and an agent can read; tags, comments, scripts and styles go;
//! character references are decoded.
//!
//! Ticket text is untrusted (`PRD-07`): this never fails and takes time in
//! step with its input, whatever it is given. Control characters are left
//! for `text::multi_line`, which the caller applies after.

use std::borrow::Cow;

/// Whether `text` is HTML rather than plain text or a markup of its own:
/// it starts with a tag and closes one.
pub fn looks_like_html(text: &str) -> bool {
    let text = text.trim_start();
    text.starts_with('<')
        && text.as_bytes().get(1).is_some_and(u8::is_ascii_alphabetic)
        && (text.contains("</") || text.contains("/>") || text.contains("<br"))
}

/// `html` as plain text.
pub fn to_text(html: &str) -> String {
    let mut out = Out::default();
    let mut rest = html;
    while !rest.is_empty() {
        let Some(at) = rest.find('<') else {
            out.text(rest);
            break;
        };
        out.text(&rest[..at]);
        rest = &rest[at..];
        match token(rest) {
            Some((Token::Open { name, attributes }, after)) => {
                rest = after;
                if SKIPPED.contains(&name.as_str()) {
                    // What these hold is never text to show.
                    rest = find_ignoring_case(rest, &format!("</{name}"))
                        .map_or("", |end| &rest[end..]);
                } else {
                    out.open(&name, &attributes);
                }
            }
            Some((Token::Close { name }, after)) => {
                out.close(&name);
                rest = after;
            }
            Some((Token::Text(text), after)) => {
                out.text(text);
                rest = after;
            }
            Some((Token::Skip, after)) => rest = after,
            None => {
                // Not a tag: a `<` in the text.
                out.text("<");
                rest = &rest[1..];
            }
        }
    }
    out.finish()
}

/// Elements whose content is not text.
const SKIPPED: &[&str] = &["script", "style", "head", "title", "template"];

/// What a `<` starts.
enum Token<'a> {
    Open {
        name: String,
        attributes: Vec<(String, String)>,
    },
    Close {
        name: String,
    },
    /// A CDATA section's text.
    Text(&'a str),
    /// A comment, a doctype or a processing instruction.
    Skip,
}

/// The token at the start of `input`, which starts with `<`, and what
/// follows it; `None` if it is no tag.
fn token(input: &str) -> Option<(Token<'_>, &str)> {
    if let Some(comment) = input.strip_prefix("<!--") {
        let end = comment.find("-->").map_or(comment.len(), |at| at + 3);
        return Some((Token::Skip, &comment[end..]));
    }
    if let Some(data) = input.strip_prefix("<![CDATA[") {
        return Some(match data.find("]]>") {
            Some(at) => (Token::Text(&data[..at]), &data[at + 3..]),
            None => (Token::Text(data), ""),
        });
    }
    let bytes = input.as_bytes();
    match bytes.get(1)? {
        b'!' | b'?' => {
            let end = input.find('>').map_or(input.len(), |at| at + 1);
            Some((Token::Skip, &input[end..]))
        }
        b'/' => {
            let name = name_at(&input[2..]);
            if name.is_empty() {
                return None;
            }
            let end = input.find('>')?;
            Some((
                Token::Close {
                    name: name.to_ascii_lowercase(),
                },
                &input[end + 1..],
            ))
        }
        first if first.is_ascii_alphabetic() => {
            let name = name_at(&input[1..]);
            let (attributes, end) = attributes(input, 1 + name.len())?;
            Some((
                Token::Open {
                    name: name.to_ascii_lowercase(),
                    attributes,
                },
                &input[end..],
            ))
        }
        _ => None,
    }
}

/// The tag or attribute name at the start of `text`.
fn name_at(text: &str) -> &str {
    let end = text
        .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, ':' | '-' | '_')))
        .unwrap_or(text.len());
    &text[..end]
}

/// The attributes of the tag in `input` from `at`, and where the tag ends;
/// `None` if it never does.
fn attributes(input: &str, mut at: usize) -> Option<(Vec<(String, String)>, usize)> {
    let bytes = input.as_bytes();
    let mut found = Vec::new();
    let skip_spaces = |at: &mut usize| {
        while bytes.get(*at).is_some_and(u8::is_ascii_whitespace) {
            *at += 1;
        }
    };
    loop {
        skip_spaces(&mut at);
        match bytes.get(at)? {
            b'>' => return Some((found, at + 1)),
            b'/' if bytes.get(at + 1) == Some(&b'>') => return Some((found, at + 2)),
            b'/' => at += 1,
            _ => {
                // Stops only at ASCII, so never inside a character.
                let start = at;
                while bytes
                    .get(at)
                    .is_some_and(|b| !b.is_ascii_whitespace() && !matches!(b, b'=' | b'>' | b'/'))
                {
                    at += 1;
                }
                let name = input[start..at].to_ascii_lowercase();
                skip_spaces(&mut at);
                let mut value = String::new();
                if bytes.get(at) == Some(&b'=') {
                    at += 1;
                    skip_spaces(&mut at);
                    if let Some(&quote @ (b'"' | b'\'')) = bytes.get(at) {
                        let start = at + 1;
                        let end = start + input[start..].find(char::from(quote))?;
                        value = decode(&input[start..end]).into_owned();
                        at = end + 1;
                    } else {
                        let start = at;
                        while bytes
                            .get(at)
                            .is_some_and(|b| !b.is_ascii_whitespace() && *b != b'>')
                        {
                            at += 1;
                        }
                        value = decode(&input[start..at]).into_owned();
                    }
                }
                found.push((name, value));
            }
        }
    }
}

/// `text` with its character references decoded; one Muster does not know
/// is kept as written.
fn decode(text: &str) -> Cow<'_, str> {
    if !text.contains('&') {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let named = rest[1..]
            .find(';')
            .filter(|end| *end <= 32)
            .and_then(|end| Some((reference(&rest[1..=end])?, end + 2)));
        if let Some((decoded, length)) = named {
            out.push(decoded);
            rest = &rest[length..];
        } else {
            out.push('&');
            rest = &rest[1..];
        }
    }
    out.push_str(rest);
    Cow::Owned(out)
}

/// The character a reference names: `#39`, `#x27`, `amp`.
fn reference(name: &str) -> Option<char> {
    if let Some(number) = name.strip_prefix('#') {
        let code = match number.strip_prefix(['x', 'X']) {
            Some(hex) => u32::from_str_radix(hex, 16).ok()?,
            None => number.parse().ok()?,
        };
        return char::from_u32(code).filter(|c| *c != '\0');
    }
    Some(match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => '\u{a0}',
        "ndash" => '–',
        "mdash" => '—',
        "hellip" => '…',
        "lsquo" => '‘',
        "rsquo" => '’',
        "ldquo" => '“',
        "rdquo" => '”',
        "laquo" => '«',
        "raquo" => '»',
        "bull" => '•',
        "middot" => '·',
        "times" => '×',
        "deg" => '°',
        "copy" => '©',
        "reg" => '®',
        "trade" => '™',
        "euro" => '€',
        "rarr" => '→',
        "larr" => '←',
        _ => return None,
    })
}

/// Whether `c` is white space that collapses; a no-break space does not.
fn collapses(c: char) -> bool {
    c.is_whitespace() && c != '\u{a0}'
}

/// The first place `needle`, in ASCII, is in `haystack`, ignoring case.
fn find_ignoring_case(haystack: &str, needle: &str) -> Option<usize> {
    let (haystack, needle) = (haystack.as_bytes(), needle.as_bytes());
    haystack
        .windows(needle.len())
        .position(|window| window.eq_ignore_ascii_case(needle))
}

/// The text being written.
#[derive(Default)]
struct Out {
    text: String,
    /// A space is owed before the next word.
    space: bool,
    /// Line breaks owed before the next text: one ends the line, two
    /// leave a blank line.
    breaks: usize,
    /// A list item's marker was just written: what opens inside the item
    /// starts on its line.
    marker: bool,
    /// How deep in `<pre>`: white space is kept there.
    pre: usize,
    /// A `<pre>` just began: its first line break is not text.
    pre_began: bool,
    /// The lists open: `None` bulleted, `Some(n)` numbered, at `n`.
    lists: Vec<Option<u32>>,
    /// The links open: where each one's text starts, and where it goes.
    links: Vec<(usize, Option<String>)>,
    /// A table cell was written in this row.
    cell: bool,
}

impl Out {
    /// Writes what is owed before more text.
    fn flush(&mut self) {
        if self.text.is_empty() {
            self.breaks = 0;
            self.space = false;
            return;
        }
        if self.breaks > 0 {
            let kept = self.text.trim_end_matches(' ').len();
            self.text.truncate(kept);
            let have = self.text.chars().rev().take_while(|c| *c == '\n').count();
            for _ in have..self.breaks {
                self.text.push('\n');
            }
        } else if self.space && !self.text.ends_with([' ', '\n']) {
            self.text.push(' ');
        }
        self.breaks = 0;
        self.space = false;
    }

    fn write(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.flush();
        self.text.push_str(text);
        self.marker = false;
    }

    /// Text between tags.
    fn text(&mut self, raw: &str) {
        let decoded = decode(raw);
        if self.pre > 0 {
            let mut text = decoded.as_ref();
            // `<pre><code>` puts nothing between them.
            if !text.is_empty() && std::mem::take(&mut self.pre_began) {
                text = text.strip_prefix("\r\n").unwrap_or(text);
                text = text.strip_prefix('\n').unwrap_or(text);
            }
            self.write(text);
            return;
        }
        let mut word = String::new();
        for c in decoded.chars() {
            if collapses(c) {
                if !word.is_empty() {
                    self.write(&word);
                    word.clear();
                }
                self.space = true;
            } else {
                word.push(c);
            }
        }
        self.write(&word);
    }

    /// Owes `breaks` line breaks, unless a list item's marker was just
    /// written.
    fn block(&mut self, breaks: usize) {
        if !self.marker {
            self.breaks = self.breaks.max(breaks);
        }
    }

    /// A paragraph's break: a blank line, or in a list a new line.
    fn paragraph(&self) -> usize {
        if self.lists.is_empty() { 2 } else { 1 }
    }

    fn open(&mut self, name: &str, attributes: &[(String, String)]) {
        let attribute = |wanted: &str| {
            attributes
                .iter()
                .find(|(name, _)| name == wanted)
                .map(|(_, value)| value.trim())
        };
        match name {
            "p" | "blockquote" | "dl" | "figure" | "address" | "section" | "article" | "header"
            | "footer" | "main" | "nav" | "aside" => self.block(self.paragraph()),
            "div" | "dt" | "dd" | "figcaption" | "caption" => self.block(1),
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                self.block(2);
                let level = usize::from(name.as_bytes()[1] - b'0');
                self.write(&format!("{} ", "#".repeat(level)));
            }
            "br" if self.pre > 0 => self.text.push('\n'),
            "br" => self.breaks = (self.breaks + 1).min(2),
            "hr" => {
                self.block(2);
                self.write("---");
                self.block(2);
            }
            "ul" | "ol" => {
                self.block(self.paragraph());
                self.lists.push((name == "ol").then(|| {
                    attribute("start")
                        .and_then(|start| start.parse().ok())
                        .unwrap_or(1)
                }));
            }
            "li" => {
                self.marker = false;
                self.block(1);
                let indent = "  ".repeat(self.lists.len().saturating_sub(1));
                let marker = match self.lists.last_mut() {
                    Some(Some(number)) => {
                        let marker = format!("{number}. ");
                        *number = number.saturating_add(1);
                        marker
                    }
                    _ => "- ".to_owned(),
                };
                self.write(&format!("{indent}{marker}"));
                self.marker = true;
            }
            "pre" => {
                self.block(2);
                self.write("```");
                self.block(1);
                self.pre += 1;
                self.pre_began = true;
            }
            "code" if self.pre == 0 => self.write("`"),
            "a" => {
                let href = attribute("href")
                    .filter(|href| {
                        let lower = href.to_ascii_lowercase();
                        ["http://", "https://", "mailto:"]
                            .iter()
                            .any(|scheme| lower.starts_with(scheme))
                    })
                    .map(str::to_owned);
                self.links.push((self.text.len(), href));
            }
            "img" => {
                let alt = attribute("alt").unwrap_or_default();
                if alt.is_empty() {
                    self.write("[image]");
                } else {
                    self.write(&format!("[image: {alt}]"));
                }
            }
            "table" => {
                self.block(2);
                self.cell = false;
            }
            "tr" => {
                self.block(1);
                self.cell = false;
            }
            "td" | "th" => {
                if self.cell {
                    self.space = false;
                    self.write(" | ");
                }
                self.cell = true;
            }
            _ => {}
        }
    }

    fn close(&mut self, name: &str) {
        match name {
            "p" | "blockquote" | "dl" | "figure" | "address" | "section" | "article" | "header"
            | "footer" | "main" | "nav" | "aside" => self.block(self.paragraph()),
            "div" | "dt" | "dd" | "figcaption" | "caption" | "li" | "tr" => self.block(1),
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "table" => self.block(2),
            "ul" | "ol" => {
                self.lists.pop();
                self.block(self.paragraph());
            }
            "pre" if self.pre > 0 => {
                self.pre -= 1;
                self.pre_began = false;
                if !self.text.ends_with('\n') {
                    self.text.push('\n');
                }
                self.write("```");
                self.block(2);
            }
            "code" if self.pre == 0 => {
                self.space = false;
                self.write("`");
            }
            "a" => {
                let Some((start, href)) = self.links.pop() else {
                    return;
                };
                let Some(href) = href else { return };
                let start = start.min(self.text.len());
                let label = self.text[start..].trim();
                let address = href.strip_prefix("mailto:").unwrap_or(&href);
                if label.is_empty() {
                    self.write(&href);
                } else if label.trim_end_matches('/') != address.trim_end_matches('/') {
                    let gap = if self.breaks > 0 { "" } else { " " };
                    self.space = false;
                    self.write(&format!("{gap}({href})"));
                }
            }
            _ => {}
        }
    }

    fn finish(self) -> String {
        let text = self.text.replace('\u{a0}', " ");
        let lines: Vec<&str> = text.lines().map(str::trim_end).collect();
        lines.join("\n").trim().to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jira_s_paragraphs_links_and_mentions_read_as_text() {
        let html = "<p data-local-id=\"0e43\"><a href=\"https://acme.atlassian.net/browse/APP-6\" \
                    data-card-appearance=\"inline\">https://acme.atlassian.net/browse/APP-6</a>  \
                    task&#039;ı bitince menü gelir.</p><p data-local-id=\"faa9\"><span \
                    data-type=\"mention\" data-user-id=\"7120:f0bf\">@Ada Lovelace</span>  ile \
                    konuşulmalı.</p>";
        assert!(looks_like_html(html));
        assert_eq!(
            to_text(html),
            "https://acme.atlassian.net/browse/APP-6 task'ı bitince menü gelir.\n\n\
             @Ada Lovelace ile konuşulmalı."
        );
    }

    #[test]
    fn lists_keep_their_marks_numbers_and_depth() {
        let html = "<p>Steps:</p><ol><li><p>Open it</p></li><li>Check:<ul><li>this</li>\
                    <li>that</li></ul></li></ol><ol start=\"7\"><li>seventh</li></ol><p>Done.</p>";
        assert_eq!(
            to_text(html),
            "Steps:\n\n1. Open it\n2. Check:\n  - this\n  - that\n\n7. seventh\n\nDone."
        );
    }

    #[test]
    fn headings_breaks_rules_code_and_tables() {
        let html = "<h2>Why</h2><p>One<br>two<br/><br/>three</p><hr><p>Run <code>make  \
                    test</code> first:</p><pre><code>\nfn main() {\n    go();\n}\n</code></pre>\
                    <table><tr><th>Name</th><th>Size</th></tr><tr><td>a</td><td>1</td></tr></table>";
        assert_eq!(
            to_text(html),
            "## Why\n\nOne\ntwo\n\nthree\n\n---\n\nRun `make test` first:\n\n```\nfn main() {\n    \
             go();\n}\n```\n\nName | Size\na | 1"
        );
    }

    #[test]
    fn links_say_where_they_go_unless_their_text_does() {
        assert_eq!(
            to_text("<p>See <a href=\"https://x.example/doc\">the doc</a>.</p>"),
            "See the doc (https://x.example/doc)."
        );
        assert_eq!(
            to_text("<p><a href=\"mailto:ada@example.com\">ada@example.com</a></p>"),
            "ada@example.com"
        );
        assert_eq!(
            to_text("<p><a href='https://x.example/'></a></p>"),
            "https://x.example/"
        );
        // Only addresses worth following are given.
        assert_eq!(
            to_text("<p><a href=\"javascript:alert(1)\">click</a> <a href=/browse/X>X</a></p>"),
            "click X"
        );
        assert_eq!(
            to_text("<p><img src=\"a.png\" alt=\"login page\"> <img src=b.png></p>"),
            "[image: login page] [image]"
        );
    }

    #[test]
    fn what_is_not_text_goes_and_references_are_decoded() {
        let html = "<!DOCTYPE html><html><head><title>t</title><style>p{}</style></head><body>\
                    <!-- a note --><SCRIPT>alert('x')</script><P CLASS=x>a &amp; b &lt;c&gt; \
                    &quot;d&quot; &#x27;e&#39; &nbsp;f&mdash;g &unknown; &amp</P>\
                    <ac:plain-text-body><![CDATA[x < y]]></ac:plain-text-body></body></html>";
        assert_eq!(
            to_text(html),
            "a & b <c> \"d\" 'e'  f—g &unknown; &amp\n\nx < y"
        );
    }

    #[test]
    fn plain_text_and_stray_brackets_are_left_as_they_are() {
        for plain in [
            "Plain text",
            "a <b> c",
            "wiki *bold*",
            "<not closed",
            "1 < 2",
        ] {
            assert!(!looks_like_html(plain), "{plain}");
        }
        assert!(looks_like_html("  <div>x</div>"));
        assert!(looks_like_html("<p>one<br>two"));
        assert_eq!(to_text("<p>1 < 2 and 3 <= 4</p>"), "1 < 2 and 3 <= 4");
        assert_eq!(to_text("<p>a <b"), "a <b");
        assert_eq!(
            to_text("<p class=\"never closed>x"),
            "<p class=\"never closed>x"
        );
    }

    #[test]
    fn anything_at_all_is_read_without_failing() {
        for input in [
            "<",
            "<<",
            "<>",
            "</",
            "</>",
            "<!",
            "<!--",
            "<![CDATA[",
            "&",
            "&#",
            "&#;",
            "&#x;",
            "&#xFFFFFFFF;",
            "&#0;",
            "&#55296;",
            "<ü>",
            "&ü;",
            "<a href=\"ü",
            "<a href=ü>ü</a>",
            "</a></li></ul></pre></code>",
            "<li><li><ol><li>",
            "<pre><pre>x</pre>",
            "ü<br>ü",
            "<td>x<td>y",
            "<script>never closed",
            "<p\u{0}>x</p>",
            "<a href='x'>",
        ] {
            let _ = to_text(input);
            let _ = looks_like_html(input);
        }
        assert_eq!(to_text("<script>never closed"), "");
        assert_eq!(to_text("&#0;x"), "&#0;x");
    }
}

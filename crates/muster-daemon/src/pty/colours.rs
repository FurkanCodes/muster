//! A program's questions about its terminal's colours, and their answers.
//!
//! Programs ask for the default foreground and background (OSC 10 and 11)
//! and for palette colours (OSC 4) to pick a light or a dark theme, as in
//! `ESC ] 11 ; ? BEL`. The answer is the colour in X11's form, ended as the
//! question was: `ESC ] 11 ; rgb:1e1e/1e1e/2e2e BEL`. They are answered
//! from the colours of the person's terminal, which a UI tells the daemon;
//! a colour no UI has said is not answered, as before there were any.

use muster_core::colours::TerminalColours;

use super::queries::number;

/// A colour a program asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Colour {
    Foreground,
    Background,
    Palette(u8),
}

/// How the question ended, which its answer ends with too.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Ending {
    /// BEL.
    Bel,
    /// ST, `ESC \`.
    St,
}

/// The colours an OSC's body asks for, in order. `10;?;?` asks for the
/// foreground and then the background, as xterm reads it (each field after
/// the first is the next colour's); `4;1;?;9;?` for palette colours 1 and
/// 9. A colour set rather than asked for is passed over: the person's
/// terminal draws in its own colours, whatever a program sets.
pub(crate) fn questions(body: &[u8]) -> Vec<Colour> {
    let mut fields = body.split(|&byte| byte == b';');
    let Some(first) = fields.next().and_then(number) else {
        return Vec::new();
    };
    match first {
        10 | 11 => fields
            .zip(first..)
            .filter(|(field, _)| *field == b"?")
            .filter_map(|(_, asked)| match asked {
                10 => Some(Colour::Foreground),
                11 => Some(Colour::Background),
                _ => None,
            })
            .collect(),
        4 => {
            let fields: Vec<&[u8]> = fields.collect();
            fields
                .chunks(2)
                .filter_map(|pair| match pair {
                    [index, b"?"] => number(index)
                        .and_then(|index| u8::try_from(index).ok())
                        .map(Colour::Palette),
                    _ => None,
                })
                .collect()
        }
        _ => Vec::new(),
    }
}

/// The answer to a question about `colour`, if the person's terminal said
/// it.
pub(crate) fn answer(colour: Colour, ending: Ending, colours: &TerminalColours) -> Option<Vec<u8>> {
    let (asked, rgb) = match colour {
        Colour::Foreground => ("10".to_owned(), colours.foreground?),
        Colour::Background => ("11".to_owned(), colours.background?),
        Colour::Palette(index) => (format!("4;{index}"), colours.palette(index)?),
    };
    let end = match ending {
        Ending::Bel => "\x07",
        Ending::St => "\x1b\\",
    };
    Some(format!("\x1b]{asked};{}{end}", rgb.spec()).into_bytes())
}

#[cfg(test)]
mod tests {
    use muster_core::colours::Rgb;

    use super::super::queries::{QueryScanner, Reply};
    use super::*;

    fn colours() -> TerminalColours {
        let grey = |level| {
            Some(Rgb {
                red: level,
                green: level,
                blue: level,
            })
        };
        TerminalColours {
            foreground: grey(0xd0d0),
            background: Some(Rgb {
                red: 0x1e1e,
                green: 0x1e1e,
                blue: 0x2e2e,
            }),
            palette: (0..16).map(|n| grey(n * 0x1000)).collect(),
        }
    }

    /// What the scanner asks of `output`, answered from `colours`.
    fn answered(output: &[u8], colours: &TerminalColours) -> Vec<u8> {
        QueryScanner::default()
            .scan(output)
            .replies
            .into_iter()
            .filter_map(|reply| match reply {
                Reply::Colour { colour, ending } => answer(colour, ending, colours),
                Reply::Bytes(bytes) => Some(bytes),
                Reply::CursorPosition { .. } | Reply::PixelSize { .. } => None,
            })
            .flatten()
            .collect()
    }

    #[test]
    fn the_default_colours_are_answered_ended_as_asked() {
        let colours = colours();
        assert_eq!(
            answered(b"\x1b]11;?\x07", &colours),
            b"\x1b]11;rgb:1e1e/1e1e/2e2e\x07"
        );
        assert_eq!(
            answered(b"\x1b]10;?\x1b\\", &colours),
            b"\x1b]10;rgb:d0d0/d0d0/d0d0\x1b\\"
        );
        // Both in one question: the foreground, then the background.
        assert_eq!(
            answered(b"\x1b]10;?;?\x07", &colours),
            b"\x1b]10;rgb:d0d0/d0d0/d0d0\x07\x1b]11;rgb:1e1e/1e1e/2e2e\x07"
        );
    }

    #[test]
    fn palette_colours_are_answered_for_the_sixteen_said() {
        let colours = colours();
        assert_eq!(
            answered(b"\x1b]4;1;?;15;?\x1b\\", &colours),
            b"\x1b]4;1;rgb:1000/1000/1000\x1b\\\x1b]4;15;rgb:f000/f000/f000\x1b\\"
        );
        // Past the sixteen, and set rather than asked: nothing.
        assert_eq!(answered(b"\x1b]4;16;?\x07", &colours), b"");
        assert_eq!(answered(b"\x1b]4;1;rgb:00/00/00\x07", &colours), b"");
        assert_eq!(answered(b"\x1b]11;#000000\x07", &colours), b"");
    }

    #[test]
    fn with_no_colours_from_any_ui_nothing_is_answered() {
        let none = TerminalColours::default();
        assert_eq!(
            answered(b"\x1b]10;?\x07\x1b]11;?\x07\x1b]4;0;?\x07", &none),
            b""
        );
    }

    #[test]
    fn colour_questions_keep_their_place_among_the_others() {
        let colours = colours();
        let answers = answered(b"\x1b[c\x1b]11;?\x07\x1b[5n", &colours);
        assert_eq!(answers, b"\x1b[?6c\x1b]11;rgb:1e1e/1e1e/2e2e\x07\x1b[0n");
    }

    #[test]
    fn other_strings_and_broken_questions_ask_nothing() {
        let colours = colours();
        for noise in [
            // A title, a progress report, a copy, a link.
            &b"\x1b]0;11;?\x07"[..],
            b"\x1b]9;4;3\x07",
            b"\x1b]52;c;aGk=\x07",
            b"\x1b]8;;https://example.com\x1b\\",
            // Cancelled, cut off by another sequence, and far too long.
            b"\x1b]11;?\x18",
            b"\x1b]11;?\x1b[m",
            &[b"\x1b]11;?".as_slice(), &[b';'; 300], b"\x07"].concat(),
        ] {
            assert_eq!(answered(noise, &colours), b"", "{noise:?}");
        }
        // A sequence that cut one off is still read.
        assert_eq!(answered(b"\x1b]11;?\x1b[c", &colours), b"\x1b[?6c");
    }

    #[test]
    fn a_question_split_across_reads_is_answered() {
        let mut scanner = QueryScanner::default();
        assert!(scanner.scan(b"\x1b]1").replies.is_empty());
        assert!(scanner.scan(b"1;?\x1b").replies.is_empty());
        assert_eq!(
            scanner.scan(b"\\").replies,
            [Reply::Colour {
                colour: Colour::Background,
                ending: Ending::St
            }]
        );
    }
}

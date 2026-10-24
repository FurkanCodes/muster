//! The person's terminal's colours, which programs in sessions ask for.
//!
//! Programs ask their terminal for its default foreground and background
//! (OSC 10 and 11) and for the sixteen colours of its palette (OSC 4) to
//! choose a light or a dark theme. A session's terminal is Muster's, which
//! draws in the person's own terminal's colours, so the UI asks that one
//! as it starts and tells the daemon, which answers sessions from them.
//! Terminals say a colour in X11's form, `rgb:RRRR/GGGG/BBBB`, each channel
//! in one to four hex digits; Muster keeps it in sixteen bits and answers
//! in four digits, as xterm does.

use serde::{Deserialize, Serialize};

/// How many of a palette's colours are asked for and answered: the sixteen
/// ANSI colours, the ones the person's theme sets.
pub const PALETTE: usize = 16;

/// One colour, each channel in sixteen bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Rgb {
    pub red: u16,
    pub green: u16,
    pub blue: u16,
}

impl Rgb {
    /// Reads `rgb:R/G/B`, each channel one to four hex digits, scaled to
    /// sixteen bits as X11 does (`rgb:ff/80/00` is `ffff/8080/0000`).
    /// Anything else is not read.
    #[must_use]
    pub fn parse(spec: &[u8]) -> Option<Self> {
        let channels = spec.strip_prefix(b"rgb:")?;
        let mut parts = channels.split(|&byte| byte == b'/');
        let mut channel = || parts.next().and_then(channel);
        let rgb = Self {
            red: channel()?,
            green: channel()?,
            blue: channel()?,
        };
        parts.next().is_none().then_some(rgb)
    }

    /// The colour as xterm answers it: `rgb:RRRR/GGGG/BBBB`, lowercase.
    #[must_use]
    pub fn spec(self) -> String {
        format!("rgb:{:04x}/{:04x}/{:04x}", self.red, self.green, self.blue)
    }
}

/// One channel, one to four hex digits, scaled to sixteen bits.
fn channel(digits: &[u8]) -> Option<u16> {
    if digits.is_empty() || digits.len() > 4 || !digits.iter().all(u8::is_ascii_hexdigit) {
        return None;
    }
    let value = u32::from_str_radix(std::str::from_utf8(digits).ok()?, 16).ok()?;
    let max = (1_u32 << (4 * digits.len())) - 1;
    u16::try_from((value * 0xffff + max / 2) / max).ok()
}

/// The colours of the person's terminal, those it said when asked.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct TerminalColours {
    /// Its default text colour (OSC 10).
    #[serde(default)]
    pub foreground: Option<Rgb>,
    /// Its default background (OSC 11).
    #[serde(default)]
    pub background: Option<Rgb>,
    /// Its palette's first colours, by number (OSC 4); at most
    /// [`PALETTE`] are used.
    #[serde(default)]
    pub palette: Vec<Option<Rgb>>,
}

impl TerminalColours {
    /// Whether the terminal said any colour at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.foreground.is_none()
            && self.background.is_none()
            && self.palette.iter().all(Option::is_none)
    }

    /// Palette colour `index`, if the terminal said it.
    #[must_use]
    pub fn palette(&self, index: u8) -> Option<Rgb> {
        let index = usize::from(index);
        (index < PALETTE)
            .then(|| self.palette.get(index).copied().flatten())
            .flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_are_read_in_x11_s_form_and_scaled_to_sixteen_bits() {
        let rgb = |red, green, blue| Some(Rgb { red, green, blue });
        assert_eq!(
            Rgb::parse(b"rgb:1e1e/2020/2a2a"),
            rgb(0x1e1e, 0x2020, 0x2a2a)
        );
        assert_eq!(Rgb::parse(b"rgb:ff/80/00"), rgb(0xffff, 0x8080, 0));
        assert_eq!(Rgb::parse(b"rgb:f/8/0"), rgb(0xffff, 0x8888, 0));
        assert_eq!(Rgb::parse(b"rgb:fff/800/000"), rgb(0xffff, 0x8008, 0));
        for not_one in [
            &b"rgb:ff/80"[..],
            b"rgb:ff/80/00/00",
            b"rgb:fffff/0/0",
            b"rgb:gg/00/00",
            b"rgb://",
            b"rgba:ff/ff/ff/ff",
            b"#ff8000",
            b"",
        ] {
            assert_eq!(Rgb::parse(not_one), None, "{not_one:?}");
        }
    }

    #[test]
    fn a_colour_is_said_as_xterm_says_it() {
        let rgb = Rgb {
            red: 0x1e1e,
            green: 0x20,
            blue: 0xabcd,
        };
        assert_eq!(rgb.spec(), "rgb:1e1e/0020/abcd");
        assert_eq!(Rgb::parse(rgb.spec().as_bytes()), Some(rgb));
    }

    #[test]
    fn only_the_sixteen_colours_said_are_known() {
        let red = Rgb {
            red: 0xcdcd,
            green: 0,
            blue: 0,
        };
        let mut colours = TerminalColours::default();
        assert!(colours.is_empty());
        colours.palette = vec![None, Some(red)];
        colours.palette.resize(20, Some(red));
        assert!(!colours.is_empty());
        assert_eq!(colours.palette(0), None);
        assert_eq!(colours.palette(1), Some(red));
        assert_eq!(colours.palette(15), Some(red));
        assert_eq!(colours.palette(16), None, "past the sixteen");
    }
}

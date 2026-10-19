//! The colours the UI draws in (`Preferences::palette`). Muster's own are
//! full colour: soft text, one accent, the statuses' colours, and shades
//! taken from the terminal's own background (`app.colours`, asked as the
//! UI starts), so a selected line is a little lighter than whatever the
//! terminal is, never a grey of Muster's choosing; light where that
//! background is light. The terminal's own are its sixteen, so the UI
//! follows the person's theme, as it did before Muster had its own.

use muster_core::colours::{Rgb, TerminalColours};
use muster_core::settings::Palette;
use ratatui::style::{Color, Modifier, Style};

use crate::app::App;

/// The colours `app` draws in: the person's palette on their terminal.
pub(crate) fn of(app: &App) -> Theme {
    Theme::new(app.model.preferences().palette, &app.colours)
}

/// The colours of every screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Theme {
    pub(crate) palette: Palette,
    /// The UI's own text; `Reset` leaves the terminal's.
    pub(crate) text: Color,
    /// What says less than the text beside it: a branch, a time, a label.
    pub(crate) quiet: Style,
    /// Borders and lines that only part things.
    pub(crate) line: Color,
    /// What has the keys: the session typed in, the screen shown.
    pub(crate) accent: Color,
    /// Text on the accent.
    pub(crate) on_accent: Color,
    /// The selected line's background while its list has the keys: a
    /// shade of the accent.
    pub(crate) lit: Color,
    /// The selected line's background while the keys are elsewhere.
    pub(crate) tint: Color,
    /// An agent at work.
    pub(crate) working: Color,
    /// A session that needs the person, and what went wrong.
    pub(crate) waiting: Color,
    /// A finished turn, and what went right.
    pub(crate) done: Color,
    /// News not seen yet.
    pub(crate) news: Color,
    /// A session that has not started, or has ended.
    pub(crate) still: Color,
    /// The projects' colours, in the order they are listed: apart from the
    /// statuses' colours, so a project never reads as a status.
    pub(crate) projects: [Color; 6],
}

/// Muster's own colours on a dark terminal.
const DARK: Theme = Theme {
    palette: Palette::Muster,
    text: rgb(0xd8, 0xda, 0xe5),
    quiet: Style::new().fg(rgb(0x8e, 0x93, 0xa8)),
    line: rgb(0x3e, 0x41, 0x54),
    accent: rgb(0xb8, 0xa6, 0xf8),
    on_accent: rgb(0x17, 0x16, 0x1f),
    lit: rgb(0x3f, 0x3c, 0x5b),
    tint: rgb(0x2a, 0x2b, 0x3a),
    working: rgb(0xf2, 0xcc, 0x8f),
    waiting: rgb(0xf2, 0x8b, 0x9d),
    done: rgb(0x93, 0xd6, 0xa3),
    news: rgb(0x8a, 0xd1, 0xe8),
    still: rgb(0x7e, 0x83, 0x98),
    projects: [
        rgb(0x8c, 0xb3, 0xf7),
        rgb(0xc7, 0xa2, 0xf4),
        rgb(0x86, 0xd6, 0xcc),
        rgb(0xf0, 0xb3, 0xd8),
        rgb(0xa7, 0xb5, 0xf7),
        rgb(0x7f, 0xc5, 0xe6),
    ],
};

/// Muster's own colours on a light terminal.
const LIGHT: Theme = Theme {
    palette: Palette::Muster,
    text: rgb(0x2e, 0x30, 0x40),
    quiet: Style::new().fg(rgb(0x6a, 0x6e, 0x82)),
    line: rgb(0xc9, 0xcb, 0xd6),
    accent: rgb(0x6b, 0x4f, 0xd8),
    on_accent: rgb(0xff, 0xff, 0xff),
    lit: rgb(0xd8, 0xd1, 0xf4),
    tint: rgb(0xe8, 0xe8, 0xef),
    working: rgb(0xa8, 0x6a, 0x00),
    waiting: rgb(0xc2, 0x36, 0x56),
    done: rgb(0x23, 0x87, 0x4a),
    news: rgb(0x1a, 0x7f, 0xa3),
    still: rgb(0x80, 0x84, 0x9a),
    projects: [
        rgb(0x2f, 0x62, 0xc9),
        rgb(0x8a, 0x45, 0xc6),
        rgb(0x19, 0x86, 0x7b),
        rgb(0xb3, 0x46, 0x88),
        rgb(0x4e, 0x5b, 0xd1),
        rgb(0x1f, 0x78, 0xa6),
    ],
};

/// The terminal's own sixteen colours.
const TERMINAL: Theme = Theme {
    palette: Palette::Terminal,
    text: Color::Reset,
    quiet: Style::new().add_modifier(Modifier::DIM),
    line: Color::DarkGray,
    accent: Color::Cyan,
    on_accent: Color::Black,
    // Sixteen colours hold no shade of the accent; the frame says it.
    lit: Color::DarkGray,
    tint: Color::DarkGray,
    working: Color::Yellow,
    waiting: Color::Red,
    done: Color::Green,
    news: Color::Cyan,
    still: Color::Gray,
    projects: [
        Color::Blue,
        Color::Magenta,
        Color::Cyan,
        Color::LightBlue,
        Color::LightMagenta,
        Color::LightCyan,
    ],
};

/// How far the selected line's background goes from the terminal's
/// towards Muster's text, in hundredths: a shade, never a colour.
const TINT_PERCENT: u16 = 9;

/// How far the selected line's background goes from the terminal's
/// towards the accent while its list has the keys.
const LIT_PERCENT: u16 = 24;

impl Theme {
    /// The colours `palette` draws in, on a terminal of `colours`.
    pub(crate) fn new(palette: Palette, colours: &TerminalColours) -> Self {
        match palette {
            Palette::Terminal => TERMINAL,
            Palette::Muster => {
                let Some(background) = colours.background else {
                    // Most terminals are dark; one that does not say is
                    // taken for one.
                    return DARK;
                };
                let base = if luminance(background) > 0.5 {
                    LIGHT
                } else {
                    DARK
                };
                Self {
                    lit: mix(background, base.accent, LIT_PERCENT),
                    tint: mix(background, base.text, TINT_PERCENT),
                    ..base
                }
            }
        }
    }

    /// The selected tab's look: lit in the accent, or the terminal's own
    /// reversed.
    pub(crate) fn tab(&self) -> Style {
        match self.palette {
            Palette::Muster => Style::new().fg(self.on_accent).bg(self.accent).bold(),
            Palette::Terminal => Style::new().reversed().bold(),
        }
    }
}

/// `colour` as the person's terminal shows it, where that can be told:
/// one of Muster's own, or one of the terminal's that it said (its
/// background for `Reset`, its palette for the sixteen).
pub(crate) fn shown(colour: Color, colours: &TerminalColours) -> Option<[u8; 3]> {
    let index = match colour {
        Color::Rgb(red, green, blue) => return Some([red, green, blue]),
        Color::Reset => return colours.background.map(eight_bits),
        Color::Black => 0,
        Color::Red => 1,
        Color::Green => 2,
        Color::Yellow => 3,
        Color::Blue => 4,
        Color::Magenta => 5,
        Color::Cyan => 6,
        Color::Gray => 7,
        Color::DarkGray => 8,
        Color::LightRed => 9,
        Color::LightGreen => 10,
        Color::LightYellow => 11,
        Color::LightBlue => 12,
        Color::LightMagenta => 13,
        Color::LightCyan => 14,
        Color::White => 15,
        Color::Indexed(index) if index < 16 => usize::from(index),
        Color::Indexed(_) => return None,
    };
    colours
        .palette
        .get(index)
        .copied()
        .flatten()
        .map(eight_bits)
}

const fn rgb(red: u8, green: u8, blue: u8) -> Color {
    Color::Rgb(red, green, blue)
}

/// A colour's channels in eight bits.
fn eight_bits(colour: Rgb) -> [u8; 3] {
    [colour.red, colour.green, colour.blue]
        .map(|channel| u8::try_from(channel >> 8).unwrap_or(u8::MAX))
}

/// How light `colour` is, from 0 (black) to 1 (white), as the eye sees it.
fn luminance(colour: Rgb) -> f64 {
    let [red, green, blue] = eight_bits(colour).map(f64::from);
    (0.2126 * red + 0.7152 * green + 0.0722 * blue) / 255.0
}

/// `from`, `percent` of the way to `to`.
fn mix(from: Rgb, to: Color, percent: u16) -> Color {
    let Color::Rgb(red, green, blue) = to else {
        return to;
    };
    let channel = |from: u8, to: u8| {
        let (from, to) = (u16::from(from), u16::from(to));
        let mixed = (from * (100 - percent) + to * percent + 50) / 100;
        u8::try_from(mixed).unwrap_or(u8::MAX)
    };
    let [from_red, from_green, from_blue] = eight_bits(from);
    rgb(
        channel(from_red, red),
        channel(from_green, green),
        channel(from_blue, blue),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn said(background: Option<[u8; 3]>) -> TerminalColours {
        TerminalColours {
            background: background.map(|[red, green, blue]| Rgb {
                red: u16::from(red) * 257,
                green: u16::from(green) * 257,
                blue: u16::from(blue) * 257,
            }),
            ..TerminalColours::default()
        }
    }

    #[test]
    fn muster_s_own_shade_the_selection_from_the_terminal_s_background() {
        let dark = Theme::new(Palette::Muster, &said(Some([0x1e, 0x1e, 0x2e])));
        assert_eq!(dark.text, DARK.text);
        // 9% of the way from the background to the text.
        assert_eq!(dark.tint, rgb(0x2f, 0x2f, 0x3e));
        // While its list has the keys, 24% of the way to the accent.
        assert_eq!(dark.lit, rgb(0x43, 0x3f, 0x5e));
        let black = Theme::new(Palette::Muster, &said(Some([0, 0, 0])));
        assert_eq!(black.tint, rgb(0x13, 0x14, 0x15));

        let light = Theme::new(Palette::Muster, &said(Some([0xfa, 0xfa, 0xfa])));
        assert_eq!(light.text, LIGHT.text);
        assert_eq!(light.accent, LIGHT.accent);
        assert!(matches!(light.tint, Color::Rgb(red, _, _) if red < 0xfa));
        assert!(matches!(light.lit, Color::Rgb(red, _, blue) if red < blue));
    }

    #[test]
    fn a_colour_is_shown_as_the_terminal_said_it_or_not_told() {
        let mut colours = said(Some([0x1e, 0x1e, 0x2e]));
        assert_eq!(shown(rgb(1, 2, 3), &colours), Some([1, 2, 3]));
        assert_eq!(shown(Color::Reset, &colours), Some([0x1e, 0x1e, 0x2e]));
        assert_eq!(shown(Color::DarkGray, &colours), None, "not said");
        colours.palette = vec![None; 16];
        colours.palette[8] = Some(Rgb {
            red: 0x5858,
            green: 0x5b5b,
            blue: 0x7070,
        });
        assert_eq!(shown(Color::DarkGray, &colours), Some([0x58, 0x5b, 0x70]));
        assert_eq!(shown(Color::Indexed(8), &colours), Some([0x58, 0x5b, 0x70]));
        assert_eq!(shown(Color::Indexed(200), &colours), None);
        assert_eq!(shown(Color::Reset, &said(None)), None);
    }

    #[test]
    fn a_terminal_that_does_not_say_is_taken_for_a_dark_one() {
        assert_eq!(Theme::new(Palette::Muster, &said(None)), DARK);
    }

    #[test]
    fn the_terminal_s_own_are_its_sixteen_whatever_it_is() {
        for background in [None, Some([0, 0, 0]), Some([0xff, 0xff, 0xff])] {
            let theme = Theme::new(Palette::Terminal, &said(background));
            assert_eq!(theme, TERMINAL);
            assert_eq!(theme.text, Color::Reset);
            assert_eq!(theme.tab(), Style::new().reversed().bold());
        }
    }
}

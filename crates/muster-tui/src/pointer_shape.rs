//! The mouse pointer's shape over what a drag would move (`PRODUCT.md` §6
//! Sessions): arrows both ways over the line between two panels, a hand
//! over a panel's title, a closed hand while one is carried. The
//! terminal is asked with OSC 22, in the names it knows: CSS's in kitty
//! and Ghostty, the X11 cursor font's in iTerm2, which shows them while a
//! program has the mouse from 3.7 on (checked 2026-10-20 in its source:
//! `fleur` is its open hand, and it has no closed one); a terminal not
//! known to take it is never sent it.

/// A pointer shape the UI asks for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Shape {
    /// The terminal's own.
    #[default]
    Default,
    /// Arrows left and right: the line between two panels side by side.
    Columns,
    /// Arrows up and down: the line between two panels one above the other.
    Rows,
    /// An open hand: a panel's title, which a drag moves.
    Grab,
    /// A closed hand: a panel being moved.
    Grabbing,
}

/// The names a terminal knows pointer shapes by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Shapes {
    /// CSS's cursor names: kitty, Ghostty.
    Css,
    /// The X11 cursor font's: iTerm2.
    X11,
}

impl Shapes {
    /// The names the terminal the environment (`var`) names knows, if it
    /// takes OSC 22 at all: never inside tmux or screen, which do not pass
    /// it on. `MUSTER_POINTER=off` sends it nothing.
    pub(crate) fn detect(var: impl Fn(&str) -> Option<String>) -> Option<Self> {
        if var("MUSTER_POINTER").as_deref() == Some("off") {
            return None;
        }
        let term = var("TERM").unwrap_or_default();
        if var("TMUX").is_some() || term.starts_with("screen") || term.starts_with("tmux") {
            return None;
        }
        let program = var("TERM_PROGRAM").unwrap_or_default();
        if program == "iTerm.app" || var("LC_TERMINAL").as_deref() == Some("iTerm2") {
            Some(Self::X11)
        } else if program == "ghostty" || term == "xterm-kitty" || var("KITTY_WINDOW_ID").is_some()
        {
            Some(Self::Css)
        } else {
            None
        }
    }

    /// The sequence that asks for `shape`; for the terminal's own, the
    /// shape asked for before is let go of.
    pub(crate) fn sequence(self, shape: Shape) -> &'static str {
        match (self, shape) {
            (Self::Css, Shape::Default) => "\x1b]22;default\x1b\\",
            (Self::Css, Shape::Columns) => "\x1b]22;ew-resize\x1b\\",
            (Self::Css, Shape::Rows) => "\x1b]22;ns-resize\x1b\\",
            (Self::Css, Shape::Grab) => "\x1b]22;grab\x1b\\",
            (Self::Css, Shape::Grabbing) => "\x1b]22;grabbing\x1b\\",
            // An empty name gives the pointer back to iTerm2, which shows
            // its own as the mouse is reported or not.
            (Self::X11, Shape::Default) => "\x1b]22;\x1b\\",
            (Self::X11, Shape::Columns) => "\x1b]22;sb_h_double_arrow\x1b\\",
            (Self::X11, Shape::Rows) => "\x1b]22;sb_v_double_arrow\x1b\\",
            (Self::X11, Shape::Grab | Shape::Grabbing) => "\x1b]22;fleur\x1b\\",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detect(vars: &[(&str, &str)]) -> Option<Shapes> {
        Shapes::detect(|name| {
            vars.iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| (*value).to_owned())
        })
    }

    #[test]
    fn each_terminal_is_asked_in_the_names_it_knows_and_others_never() {
        assert_eq!(detect(&[("TERM_PROGRAM", "iTerm.app")]), Some(Shapes::X11));
        assert_eq!(detect(&[("LC_TERMINAL", "iTerm2")]), Some(Shapes::X11));
        assert_eq!(detect(&[("TERM_PROGRAM", "ghostty")]), Some(Shapes::Css));
        assert_eq!(detect(&[("TERM", "xterm-kitty")]), Some(Shapes::Css));
        for not_one in [
            &[("TERM_PROGRAM", "Apple_Terminal")][..],
            &[("TERM_PROGRAM", "WezTerm")],
            &[("TERM_PROGRAM", "iTerm.app"), ("TMUX", "/tmp/tmux")],
            &[("TERM_PROGRAM", "ghostty"), ("TERM", "screen-256color")],
            &[("TERM_PROGRAM", "iTerm.app"), ("MUSTER_POINTER", "off")],
            &[],
        ] {
            assert_eq!(detect(not_one), None, "{not_one:?}");
        }
    }

    #[test]
    fn a_shape_is_asked_for_with_osc_22() {
        assert_eq!(
            Shapes::Css.sequence(Shape::Columns),
            "\x1b]22;ew-resize\x1b\\"
        );
        assert_eq!(
            Shapes::X11.sequence(Shape::Columns),
            "\x1b]22;sb_h_double_arrow\x1b\\"
        );
        assert_eq!(Shapes::X11.sequence(Shape::Grab), "\x1b]22;fleur\x1b\\");
        assert_eq!(Shapes::X11.sequence(Shape::Grabbing), "\x1b]22;fleur\x1b\\");
        assert_eq!(Shapes::X11.sequence(Shape::Default), "\x1b]22;\x1b\\");
        assert_eq!(
            Shapes::Css.sequence(Shape::Default),
            "\x1b]22;default\x1b\\"
        );
    }
}

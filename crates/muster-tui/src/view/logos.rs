//! The agents' logos before each session's name (`PRODUCT.md` §6): a
//! picture where the person's terminal draws pictures inline (iTerm2's
//! image protocol, which `WezTerm` speaks too), else the agent's mark in its
//! brand colour. A picture is drawn into a cell as the protocol's escape
//! sequence, the cell saying it is as wide as the picture
//! (`CellDiffOption::ForcedWidth`) and the one it covers skipped, so the
//! terminal is sent it only when the line changes there, never on each
//! frame. Where the colour under it is known, it is laid on that colour
//! (`super::picture`), so a tinted line shows no square of the terminal's
//! plain background around it. The pictures and where they come from are
//! in `assets/logos`.

use std::num::NonZeroU16;
use std::sync::LazyLock;

pub(crate) use super::picture::Backdrop;
use base64::Engine as _;
use muster_core::harness::OwnAgent;
use ratatui::buffer::{Buffer, CellDiffOption};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::Span;

/// What the person's terminal can draw.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Graphics {
    /// Text alone: each agent's mark in its colour.
    #[default]
    Text,
    /// Pictures inline, by iTerm2's protocol (`OSC 1337 ; File`).
    Iterm,
}

impl Graphics {
    /// What the terminal the environment (`var`) names draws: pictures in
    /// iTerm2 and `WezTerm`, never inside tmux or screen, which pass them on
    /// unreliably. `MUSTER_LOGOS` (`text` or `pictures`) decides for a
    /// terminal told wrong.
    pub(crate) fn detect(var: impl Fn(&str) -> Option<String>) -> Self {
        match var("MUSTER_LOGOS").as_deref() {
            Some("text") => return Self::Text,
            Some("pictures") => return Self::Iterm,
            _ => {}
        }
        let term = var("TERM").unwrap_or_default();
        if var("TMUX").is_some() || term.starts_with("screen") || term.starts_with("tmux") {
            return Self::Text;
        }
        let program = var("TERM_PROGRAM").unwrap_or_default();
        if matches!(program.as_str(), "iTerm.app" | "WezTerm")
            || var("LC_TERMINAL").as_deref() == Some("iTerm2")
        {
            Self::Iterm
        } else {
            Self::Text
        }
    }
}

/// An agent with a logo: each of Muster's own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Logo(OwnAgent);

impl Logo {
    /// The logo of the agent the harness `harness` runs; the review is
    /// Claude Code's.
    fn of(harness: &str) -> Option<Self> {
        let agent = match harness {
            muster_core::harness::REVIEW => Some(OwnAgent::Claude),
            harness => OwnAgent::of(harness),
        };
        agent.map(Self)
    }

    /// Its mark in text, and its brand colour.
    const fn glyph(self) -> (&'static str, Color) {
        match self.0 {
            OwnAgent::Claude => ("✻", Color::Rgb(0xD9, 0x77, 0x57)),
            OwnAgent::Codex => ("◎", Color::Rgb(0xA3, 0xA3, 0xA3)),
            OwnAgent::Gemini => ("✦", Color::Rgb(0x8E, 0x75, 0xB2)),
            OwnAgent::OpenCode => ("▣", Color::Rgb(0x9A, 0x94, 0x94)),
            OwnAgent::Copilot => ("◉", Color::Rgb(0x82, 0x50, 0xDF)),
        }
    }

    /// Its name, as its files are called.
    const fn name(self) -> &'static str {
        self.0.id()
    }

    /// Its pixels, unpacked from its picture by `scripts/render-logos.sh`:
    /// red, green, blue and alpha, `picture::SIDE` square.
    fn pixels(self) -> &'static [u8] {
        match self.0 {
            OwnAgent::Claude => include_bytes!("../../assets/logos/claude.rgba"),
            OwnAgent::Codex => include_bytes!("../../assets/logos/codex.rgba"),
            OwnAgent::Gemini => include_bytes!("../../assets/logos/gemini.rgba"),
            OwnAgent::OpenCode => include_bytes!("../../assets/logos/opencode.rgba"),
            OwnAgent::Copilot => include_bytes!("../../assets/logos/copilot.rgba"),
        }
    }

    fn picture(self) -> &'static [u8] {
        match self.0 {
            OwnAgent::Claude => include_bytes!("../../assets/logos/claude.png"),
            OwnAgent::Codex => include_bytes!("../../assets/logos/codex.png"),
            OwnAgent::Gemini => include_bytes!("../../assets/logos/gemini.png"),
            OwnAgent::OpenCode => include_bytes!("../../assets/logos/opencode.png"),
            OwnAgent::Copilot => include_bytes!("../../assets/logos/copilot.png"),
        }
    }

    /// The escape sequence drawing it two cells wide and one high, the
    /// cursor left where it was, made once.
    fn sequence(self) -> &'static str {
        static SEQUENCES: LazyLock<[String; 5]> =
            LazyLock::new(|| OwnAgent::ALL.map(|agent| iterm_sequence(Logo(agent).picture())));
        let at = match self.0 {
            OwnAgent::Claude => 0,
            OwnAgent::Codex => 1,
            OwnAgent::Gemini => 2,
            OwnAgent::OpenCode => 3,
            OwnAgent::Copilot => 4,
        };
        &SEQUENCES[at]
    }
}

/// iTerm2's sequence drawing the PNG `png` in two cells, one high.
fn iterm_sequence(png: &[u8]) -> String {
    format!(
        "\x1b]1337;File=inline=1;size={};width=2;height=1;preserveAspectRatio=1;\
         doNotMoveCursor=1:{}\x07",
        png.len(),
        base64::engine::general_purpose::STANDARD.encode(png)
    )
}

/// The cells a session's mark takes on its line, with the space after it.
pub(crate) const fn width(graphics: Graphics) -> usize {
    match graphics {
        Graphics::Text => 2,
        Graphics::Iterm => 3,
    }
}

/// A session's mark on its line, [`width`] cells: the agent's glyph in its
/// colour, or room for its picture where the terminal draws them. A shell
/// or the checks have a plain mark of their own.
pub(crate) fn spans(graphics: Graphics, harness: &str) -> Vec<Span<'static>> {
    // After a mark of one cell, the rest of its room.
    let gap = match graphics {
        Graphics::Text => " ",
        Graphics::Iterm => "  ",
    };
    let logo = Logo::of(harness);
    if graphics == Graphics::Iterm && logo.is_some() {
        // Room for the picture, drawn over it by `draw`.
        return vec![Span::raw("   ")];
    }
    let glyph = match (logo, harness) {
        (Some(logo), _) => {
            let (glyph, colour) = logo.glyph();
            Span::styled(glyph, Style::default().fg(colour))
        }
        (None, "shell") => Span::from("❯").dim(),
        (None, "checks") => Span::from("✔").dim(),
        (None, _) => Span::from("•").dim(),
    };
    vec![glyph, Span::raw(gap)]
}

/// Draws `harness`'s picture into `buf` at `x`, `y`, over the room
/// [`spans`] left there, where the terminal draws pictures: laid on
/// `backdrop` where the colour under it is known, else as it is.
pub(crate) fn draw(
    buf: &mut Buffer,
    graphics: Graphics,
    harness: &str,
    (x, y): (u16, u16),
    backdrop: Option<Backdrop>,
) {
    let Some(logo) = Logo::of(harness).filter(|_| graphics == Graphics::Iterm) else {
        return;
    };
    let area = buf.area;
    if y >= area.bottom() || x.saturating_add(1) >= area.right() || x < area.left() {
        return;
    }
    let laid = backdrop
        .map(|backdrop| super::picture::kept(logo.name(), logo.pixels(), backdrop, iterm_sequence));
    if let Some(cell) = buf.cell_mut((x, y)) {
        cell.set_symbol(laid.as_deref().unwrap_or_else(|| logo.sequence()))
            .set_diff_option(CellDiffOption::ForcedWidth(
                NonZeroU16::MIN.saturating_add(1),
            ));
    }
    if let Some(cell) = buf.cell_mut((x + 1, y)) {
        cell.set_diff_option(CellDiffOption::Skip);
    }
}

#[cfg(test)]
mod tests {
    use ratatui::layout::Rect;

    use super::*;

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let pairs: Vec<(String, String)> = pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect();
        move |key| {
            pairs
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value.clone())
        }
    }

    #[test]
    fn pictures_are_drawn_where_the_terminal_can_and_text_elsewhere() {
        let detect = |pairs: &[(&str, &str)]| Graphics::detect(env(pairs));
        assert_eq!(detect(&[("TERM_PROGRAM", "iTerm.app")]), Graphics::Iterm);
        assert_eq!(detect(&[("LC_TERMINAL", "iTerm2")]), Graphics::Iterm);
        assert_eq!(detect(&[("TERM_PROGRAM", "WezTerm")]), Graphics::Iterm);
        assert_eq!(
            detect(&[("TERM_PROGRAM", "Apple_Terminal")]),
            Graphics::Text
        );
        assert_eq!(detect(&[]), Graphics::Text);
        // Never through a multiplexer.
        assert_eq!(
            detect(&[
                ("TERM_PROGRAM", "iTerm.app"),
                ("TMUX", "/tmp/tmux-1/default,1,0")
            ]),
            Graphics::Text
        );
        assert_eq!(
            detect(&[("LC_TERMINAL", "iTerm2"), ("TERM", "screen-256color")]),
            Graphics::Text
        );
        // The person decides for a terminal told wrong.
        assert_eq!(
            detect(&[("TERM_PROGRAM", "iTerm.app"), ("MUSTER_LOGOS", "text")]),
            Graphics::Text
        );
        assert_eq!(detect(&[("MUSTER_LOGOS", "pictures")]), Graphics::Iterm);
    }

    #[test]
    fn each_agent_has_its_mark_in_its_colour_and_the_room_it_says() {
        for (harness, glyph) in [
            ("claude", "✻"),
            ("review", "✻"),
            ("codex", "◎"),
            ("gemini", "✦"),
            ("opencode", "▣"),
            ("copilot", "◉"),
            ("shell", "❯"),
            ("checks", "✔"),
            // An agent the person added has no logo of its own.
            ("my-agent", "•"),
        ] {
            for graphics in [Graphics::Text, Graphics::Iterm] {
                let spans = spans(graphics, harness);
                let cells: usize = spans.iter().map(Span::width).sum();
                assert_eq!(cells, width(graphics), "{harness} {graphics:?}");
            }
            assert_eq!(spans(Graphics::Text, harness)[0].content, glyph);
        }
        assert_eq!(
            spans(Graphics::Text, "claude")[0].style.fg,
            Some(Color::Rgb(0xD9, 0x77, 0x57))
        );
    }

    #[test]
    fn a_picture_is_one_cell_two_wide_sent_as_iterm2_draws_it() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 10, 2));
        draw(&mut buf, Graphics::Iterm, "gemini", (3, 1), None);
        let cell = &buf[(3, 1)];
        assert!(cell.symbol().starts_with("\x1b]1337;File=inline=1;"));
        assert!(cell.symbol().contains("width=2;height=1"));
        assert!(cell.symbol().ends_with('\x07'));
        assert_eq!(
            cell.diff_option,
            CellDiffOption::ForcedWidth(NonZeroU16::new(2).unwrap())
        );
        assert_eq!(buf[(4, 1)].diff_option, CellDiffOption::Skip);
        // Text alone, or a shell, draws nothing over the line.
        let mut plain = Buffer::empty(Rect::new(0, 0, 10, 2));
        draw(&mut plain, Graphics::Text, "gemini", (3, 1), None);
        draw(&mut plain, Graphics::Iterm, "shell", (3, 1), None);
        assert_eq!(plain, Buffer::empty(Rect::new(0, 0, 10, 2)));
        // Never past the edge.
        draw(&mut plain, Graphics::Iterm, "claude", (9, 1), None);
        assert_eq!(plain, Buffer::empty(Rect::new(0, 0, 10, 2)));
    }

    #[test]
    fn a_picture_is_sent_again_only_when_its_line_changes() {
        let area = Rect::new(0, 0, 12, 1);
        let line = |text: &str| {
            let mut buf = Buffer::empty(area);
            buf.set_string(0, 0, text, Style::default());
            draw(&mut buf, Graphics::Iterm, "claude", (2, 0), None);
            buf
        };
        let first = line("⠋    Fix it");
        let same = line("⠋    Fix it");
        let spun = line("⠙    Fix it");
        assert!(first.diff(&same).is_empty());
        let updates: Vec<(u16, u16)> = first.diff(&spun).iter().map(|(x, y, _)| (*x, *y)).collect();
        assert_eq!(updates, [(0, 0)], "only the spinner, not the picture");
    }
}

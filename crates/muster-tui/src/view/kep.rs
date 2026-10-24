//! Kep, Muster's sheepdog: Muster gathers sessions as a sheepdog musters
//! a flock. Large beside the wordmark as the UI starts and on screens with
//! nothing on them yet; small at the start of the top bar, where its face
//! says how the flock stands. In the terminal's own sixteen colours, so it
//! follows the person's theme.

use muster_core::SessionStatus;
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Paragraph, Widget, Wrap};

use super::look::WORDMARK;
use super::panel;
use super::theme::Theme;
use crate::app::{App, Link};
use crate::model::needs_attention;
use crate::motion;

/// Kep large, a character a pixel, two pixel rows to a line of the
/// terminal: `k` the coat's black (dark grey, seen on a dark theme), `b`
/// black, `w` white, `g` grey, `r` the tongue, `c` the bandana, in the
/// wordmark's cyan.
const PIXELS: [&str; 18] = [
    "......kkkkkkkkkkkk......",
    "....kkkkkkkkkkkkkkkk....",
    "..kkbkkkkkkwwkkkkkkbkk..",
    ".kkkkbkkkkkwwkkkkkbkkkk.",
    "kkkkkbkkkkwwwwkkkkbkkkkk",
    "kkkkkbkkkkwwwwkkkkbkkkkk",
    "kkkkbkkbwkwwwwkbwkkbkkkk",
    ".kkkbkkbbwwwwwwbbkkbkkk.",
    "..kk.kkkkwwwwwwkkkk.kk..",
    ".....kkkwwwwwwwwkkk.....",
    ".....kkwwwbbbbwwwkk.....",
    ".....kwwwwwbbwwwwwk.....",
    ".....gwwwwbwwbwwwwg.....",
    "......gwwwbrrbwwwg......",
    "......cccccrrccccc......",
    ".......ccccrrcccc.......",
    "........cccccccc........",
    "..........cccc..........",
];

/// Kep's size, large, in cells.
pub(crate) const WIDTH: u16 = 24;
pub(crate) const HEIGHT: u16 = 9;

/// Columns between Kep and the wordmark beside it.
const GAP: u16 = 4;

fn colour(pixel: u8) -> Option<Color> {
    match pixel {
        b'k' => Some(Color::DarkGray),
        b'b' => Some(Color::Black),
        b'w' => Some(Color::White),
        b'g' => Some(Color::Gray),
        b'r' => Some(Color::LightRed),
        b'c' => Some(Color::Cyan),
        _ => None,
    }
}

/// Kep large, from the top left of the area; nothing where it does not
/// fit.
pub(crate) struct Portrait;

impl Widget for Portrait {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        if area.width < WIDTH || area.height < HEIGHT {
            return;
        }
        for (y, rows) in (area.y..).zip(PIXELS.chunks(2)) {
            let (top, bottom) = (rows[0].as_bytes(), rows[1].as_bytes());
            for (x, (&top, &bottom)) in (area.x..).zip(top.iter().zip(bottom)) {
                let cell = &mut buffer[(x, y)];
                match (colour(top), colour(bottom)) {
                    (Some(top), Some(bottom)) => {
                        cell.set_symbol("▀").set_fg(top).set_bg(bottom);
                    }
                    (Some(top), None) => {
                        cell.set_symbol("▀").set_fg(top);
                    }
                    (None, Some(bottom)) => {
                        cell.set_symbol("▄").set_fg(bottom);
                    }
                    (None, None) => {}
                }
            }
        }
    }
}

/// How Kep looks, by how the sessions stand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mood {
    /// Something works, or the state is on its way: Kep watches.
    Watching,
    /// Watching, between blinks.
    Blinking,
    /// Something waits on the person: ears up, and a bark.
    Alert,
    /// A session has just finished.
    Glad,
    /// Nothing works or waits.
    Asleep,
    /// The daemon cannot be reached.
    Worried,
}

/// Kep's mood now. A question or a permission asked comes first; a
/// finish cheers Kep a moment before it, unseen, wants a look.
pub(crate) fn mood(app: &App) -> Mood {
    match &app.link {
        Link::Lost(_) => return Mood::Worried,
        _ if !app.model.loaded() => return Mood::Watching,
        _ => {}
    }
    let sessions = app.model.sessions();
    if sessions
        .iter()
        .any(|session| matches!(session.status, SessionStatus::NeedsYou { .. }))
    {
        return Mood::Alert;
    }
    if app.kep_glad() {
        return Mood::Glad;
    }
    if sessions.iter().any(needs_attention) {
        return Mood::Alert;
    }
    if !sessions
        .iter()
        .any(|session| session.status == SessionStatus::Running)
    {
        return Mood::Asleep;
    }
    match app.motion_step() {
        Some(step) if motion::blinking(step) => Mood::Blinking,
        _ => Mood::Watching,
    }
}

/// Kep's face for the top bar, in `FACE_WIDTH` cells: the ears, the eyes
/// either side of the nose, and a bark (pulsing at a motion `step`) or a
/// snore after it.
pub(crate) fn face(mood: Mood, step: Option<u64>) -> Vec<Span<'static>> {
    let (ears, eyes, after) = match mood {
        Mood::Watching => ("▼", "•", " "),
        Mood::Blinking => ("▼", "-", " "),
        Mood::Alert => ("▲", "•", "!"),
        Mood::Glad => ("▼", "^", " "),
        Mood::Asleep => ("▼", "-", "ᶻ"),
        Mood::Worried => ("▼", ";", " "),
    };
    let ear = Style::default().fg(Color::DarkGray).bold();
    let features = Style::default().bold();
    let after_style = match mood {
        Mood::Alert => {
            let bark = Style::default().fg(Color::Red).bold();
            step.map_or(bark, |step| motion::pulse(bark, step))
        }
        _ => Style::default().dim(),
    };
    vec![
        Span::styled(ears, ear),
        Span::styled(eyes, features),
        Span::styled("ᴥ", features),
        Span::styled(eyes, features),
        Span::styled(ears, ear),
        Span::styled(after, after_style),
    ]
}

/// The cells Kep's face takes.
pub(crate) const FACE_WIDTH: usize = 6;

/// What the welcome says under Muster's name, and whether it is bad news.
fn greeting(app: &App) -> (String, bool) {
    match &app.link {
        Link::Lost(_) => {
            let said = match &app.machine {
                Some(machine) => format!("{machine} can't be reached; reconnecting…"),
                None => "The daemon can't be reached; retrying…".to_owned(),
            };
            return (said, true);
        }
        _ if !app.model.loaded() => return ("Rounding up your sessions…".to_owned(), false),
        _ => {}
    }
    if app.model.projects().is_empty() {
        return ("Agents on your tickets, side by side.".to_owned(), false);
    }
    // As the top bar counts them.
    let sessions = app.model.sessions();
    let count = |wanted: fn(SessionStatus) -> bool| {
        sessions
            .iter()
            .filter(|session| wanted(session.status))
            .count()
    };
    let parts: Vec<String> = [
        (count(|status| status == SessionStatus::Running), "working"),
        (
            count(|status| matches!(status, SessionStatus::NeedsYou { .. })),
            "waiting on you",
        ),
        (count(|status| status == SessionStatus::Finished), "done"),
    ]
    .into_iter()
    .filter(|(count, _)| *count > 0)
    .map(|(count, word)| format!("{count} {word}"))
    .collect();
    if parts.is_empty() {
        return ("Welcome back. All quiet.".to_owned(), false);
    }
    (format!("Welcome back. {}.", parts.join(", ")), false)
}

/// The wordmark's width.
const NAME_WIDTH: u16 = 18;

/// Where a banner puts Kep, and the line under Muster's name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Layout {
    /// Kep, with the name and the line beside it.
    Beside,
    /// Kep with the name beside it, the line below them.
    Below,
    /// The name and the line alone.
    NameOnly,
}

/// The size of a banner saying `under` in `area`, and its layout: Kep
/// beside the name where there is room for both, else the name alone.
fn banner_size(area: Rect, under: &Line<'_>) -> (u16, u16, Layout) {
    let text = u16::try_from(under.width()).unwrap_or(u16::MAX);
    let name = WIDTH + GAP + NAME_WIDTH;
    if area.height >= HEIGHT && area.width >= WIDTH + GAP + text.max(NAME_WIDTH) {
        (WIDTH + GAP + text.max(NAME_WIDTH), HEIGHT, Layout::Beside)
    } else if area.width >= name && area.height >= HEIGHT + 2 {
        let width = name.max(text).min(area.width);
        let rows = text.div_ceil(width.max(1)).max(1);
        (width, (HEIGHT + 1 + rows).min(area.height), Layout::Below)
    } else {
        let width = text.max(NAME_WIDTH).min(area.width);
        (width, 5.min(area.height), Layout::NameOnly)
    }
}

/// Kep with Muster's name beside it and `under` beside or below them,
/// from the top left of `area`; the name alone where Kep does not fit.
/// The rows it took.
fn banner(frame: &mut Frame<'_>, area: Rect, under: Line<'static>, theme: Theme) -> u16 {
    let (_, height, layout) = banner_size(area, &under);
    let mut lines: Vec<Line<'static>> = WORDMARK
        .iter()
        .map(|line| Line::from(*line).fg(theme.accent).bold())
        .collect();
    if layout == Layout::NameOnly {
        lines.extend([Line::from(""), under]);
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
        return height;
    }
    frame.render_widget(Portrait, area);
    lines.splice(0..0, [Line::from(""), Line::from("")]);
    let beside = Rect {
        x: area.x + WIDTH + GAP,
        width: area.width - WIDTH - GAP,
        height: HEIGHT,
        ..area
    };
    if layout == Layout::Beside {
        lines.extend([Line::from(""), under]);
    } else {
        let below = Rect {
            y: area.y + HEIGHT + 1,
            height: height.saturating_sub(HEIGHT + 1),
            ..area
        };
        frame.render_widget(Paragraph::new(under).wrap(Wrap { trim: false }), below);
    }
    frame.render_widget(Paragraph::new(lines), beside);
    height
}

/// The welcome, filling the window as the UI starts: Kep and Muster's
/// name, centred, with how the sessions stand.
pub(crate) fn draw_welcome(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let theme = super::theme::of(app);
    let (text, bad) = greeting(app);
    let under = if bad {
        Line::from(text).fg(theme.waiting)
    } else {
        Line::from(text).dim()
    };
    let (width, height, _) = banner_size(area, &under);
    let centred = Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    };
    banner(frame, centred, under, theme);
}

/// The details pane while there is nothing to show: Kep, Muster's name
/// and `under`, then `more` below.
pub(crate) fn draw_empty_pane(
    frame: &mut Frame<'_>,
    app: &App,
    area: Rect,
    under: &str,
    more: &[&str],
) {
    let block = panel(app, " Details ", false);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let room = Rect {
        x: inner.x + 2,
        y: inner.y + 1,
        width: inner.width.saturating_sub(2),
        height: inner.height.saturating_sub(1),
    };
    let rows = banner(
        frame,
        room,
        Line::from(under.to_owned()).dim(),
        super::theme::of(app),
    );
    let below = Rect {
        y: room.y + rows + 1,
        height: room.height.saturating_sub(rows + 1),
        ..room
    };
    let lines: Vec<Line<'static>> = more
        .iter()
        .map(|line| Line::from((*line).to_owned()))
        .collect();
    frame.render_widget(
        Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false }),
        below,
    );
}

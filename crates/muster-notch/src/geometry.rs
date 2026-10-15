//! Where the notch is and where the panel goes, from what AppKit says of
//! the screens: plain numbers in AppKit's screen coordinates (points, the
//! origin at the main screen's bottom left, y up), so this is tested
//! anywhere.
//!
//! Nothing can be drawn in the notch itself, which has no pixels. At rest
//! the panel is the notch's own height with a slim side of the menu bar's
//! row either side, black like it, so what it says reads as part of the
//! notch; pointed at, the list opens down from it at that width.
//!
//! A screen with a notch has a top safe-area inset, its height, and the
//! menu bar's room either side of it (`auxiliaryTopLeftArea`,
//! `auxiliaryTopRightArea`); what is left between them is its width.

use muster_core::num::{fraction, round_u64};

/// Each slim side of the notch, where the counts are.
pub const SIDE: f64 = 30.0;
/// A listed session: its title and time, then its project and what it is
/// doing.
pub const LINE: f64 = 34.0;
/// Space above and below the list.
pub const PAD: f64 = 6.0;

/// A rectangle in screen coordinates.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn max_x(self) -> f64 {
        self.x + self.width
    }

    pub fn max_y(self) -> f64 {
        self.y + self.height
    }

    pub fn mid_x(self) -> f64 {
        self.x + self.width / 2.0
    }

    /// Whether `(x, y)` is inside, its edges included.
    pub fn contains(self, (x, y): (f64, f64)) -> bool {
        x >= self.x && x <= self.max_x() && y >= self.y && y <= self.max_y()
    }
}

/// What AppKit says of one screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Screen {
    pub frame: Rect,
    /// The safe area's top inset: the notch's height, 0 on a screen with
    /// none.
    pub safe_top: f64,
    /// The menu bar's room left and right of the notch; empty on a screen
    /// with none.
    pub left: Rect,
    pub right: Rect,
}

/// The notch: the cut-out at the top middle of the screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Notch {
    pub frame: Rect,
}

/// The first screen with a notch, and where its notch is. A notch of a
/// size no MacBook has is taken for a screen misread.
pub fn find(screens: &[Screen]) -> Option<Notch> {
    screens.iter().find_map(|screen| {
        if screen.safe_top <= 0.0 || screen.left.width <= 0.0 || screen.right.width <= 0.0 {
            return None;
        }
        let width = screen.frame.width - screen.left.width - screen.right.width;
        let height = screen.safe_top;
        if !(80.0..=400.0).contains(&width) || !(16.0..=64.0).contains(&height) {
            return None;
        }
        Some(Notch {
            frame: Rect::new(
                screen.frame.mid_x() - width / 2.0,
                screen.frame.max_y() - height,
                width,
                height,
            ),
        })
    })
}

impl Notch {
    /// The panel at rest: the notch with a slim side either side, its
    /// height and no more.
    pub fn compact(self) -> Rect {
        let notch = self.frame;
        Rect::new(
            notch.x - SIDE,
            notch.y,
            notch.width + 2.0 * SIDE,
            notch.height,
        )
    }

    /// The panel pointed at, as wide as at rest, with `lines` lines under
    /// the notch.
    pub fn expanded(self, lines: usize) -> Rect {
        let compact = self.compact();
        let height = self.frame.height + PAD + count(lines) * LINE + PAD;
        Rect::new(compact.x, compact.max_y() - height, compact.width, height)
    }

    /// The top of line `line` in the panel, from the panel's top.
    pub fn line_top(self, line: usize) -> f64 {
        self.frame.height + PAD + count(line) * LINE
    }

    /// Which line of the list is at `point` in the panel at `panel`.
    pub fn line_at(self, panel: Rect, point: (f64, f64)) -> Option<usize> {
        if !panel.contains(point) {
            return None;
        }
        let below = panel.max_y() - point.1 - self.line_top(0);
        if below < 0.0 {
            return None;
        }
        usize::try_from(round_u64((below / LINE).floor())).ok()
    }
}

fn count(lines: usize) -> f64 {
    fraction(u64::try_from(lines).unwrap_or(u64::MAX))
}

/// Whether the pointer at `point` holds the list open: it opens over the
/// panel at rest, and stays open until the pointer leaves it whole, so
/// it does not flicker at the edge.
pub fn hovering(open: bool, notch: Notch, lines: usize, point: (f64, f64)) -> bool {
    if open {
        notch.expanded(lines).contains(point)
    } else {
        notch.compact().contains(point)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 14-inch MacBook Pro's screen as AppKit says it, at its default
    /// scale: 1512 by 982 points, a notch 185 wide and 32 high.
    fn macbook() -> Screen {
        Screen {
            frame: Rect::new(0.0, 0.0, 1512.0, 982.0),
            safe_top: 32.0,
            left: Rect::new(0.0, 950.0, 663.5, 32.0),
            right: Rect::new(848.5, 950.0, 663.5, 32.0),
        }
    }

    fn display() -> Screen {
        Screen {
            frame: Rect::new(1512.0, 0.0, 2560.0, 1440.0),
            safe_top: 0.0,
            left: Rect::default(),
            right: Rect::default(),
        }
    }

    #[test]
    fn the_notch_is_found_on_the_screen_that_has_one() {
        let notch = find(&[display(), macbook()]).expect("a notch");
        assert_eq!(notch.frame, Rect::new(663.5, 950.0, 185.0, 32.0));
        assert_eq!(find(&[display()]), None);
        assert_eq!(find(&[]), None);
        // An inset without the menu bar's room either side is no notch.
        let mut misread = macbook();
        misread.right = Rect::default();
        assert_eq!(find(&[misread]), None);
        misread.right = Rect::new(0.0, 0.0, 1.0, 32.0);
        assert_eq!(find(&[misread]), None, "a notch wider than any");
    }

    #[test]
    fn at_rest_it_is_the_notch_s_height_with_slim_sides() {
        let notch = find(&[macbook()]).unwrap();
        let compact = notch.compact();
        assert_eq!(compact, Rect::new(633.5, 950.0, 245.0, 32.0));
        assert!((compact.mid_x() - 756.0).abs() < f64::EPSILON, "centred");
        for lines in [0, 3, 9] {
            let open = notch.expanded(lines);
            assert_eq!((open.x, open.width), (633.5, 245.0), "{lines} lines");
            assert!(
                (open.max_y() - 982.0).abs() < f64::EPSILON,
                "flush with the top"
            );
        }
        let open = notch.expanded(3);
        assert!((open.height - (32.0 + PAD + 3.0 * LINE + PAD)).abs() < f64::EPSILON);
    }

    #[test]
    fn a_point_is_on_the_line_drawn_there() {
        let notch = find(&[macbook()]).unwrap();
        let open = notch.expanded(3);
        let first = open.max_y() - notch.line_top(0) - 1.0;
        assert_eq!(notch.line_at(open, (756.0, first)), Some(0));
        let second = open.max_y() - notch.line_top(1) - 1.0;
        assert_eq!(notch.line_at(open, (756.0, second)), Some(1));
        // The notch's row, or off the panel: no line.
        assert_eq!(notch.line_at(open, (756.0, 970.0)), None);
        assert_eq!(notch.line_at(open, (640.0, 960.0)), None);
        assert_eq!(notch.line_at(open, (600.0, first)), None);
    }

    #[test]
    fn the_list_opens_over_the_notch_and_closes_once_left() {
        let notch = find(&[macbook()]).unwrap();
        let over_notch = (756.0, 970.0);
        let below_notch = (756.0, 930.0);
        let far = (100.0, 100.0);
        assert!(hovering(false, notch, 3, over_notch));
        assert!(!hovering(false, notch, 3, below_notch), "not open yet");
        assert!(hovering(true, notch, 3, below_notch), "over the open list");
        assert!(!hovering(true, notch, 3, far));
    }
}

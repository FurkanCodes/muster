//! Where the notch is and where the panel goes, from what AppKit says of
//! the screens: plain numbers in AppKit's screen coordinates (points, the
//! origin at the main screen's bottom left, y up), so this is tested
//! anywhere.
//!
//! Nothing can be drawn in the notch itself, which has no pixels. At rest
//! the panel is the notch's own height with a slim side of the menu bar's
//! row either side, black like it, its top corners curving out into the
//! screen's edge as the notch's own do, so what it says reads as part of
//! the notch; pointed at, the list opens down from it, wider than the
//! notch only below the menu bar's row.
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
/// The list's width once open, below the menu bar's row.
pub const LIST_WIDTH: f64 = 340.0;
/// How far the curve where the panel meets the screen's top edge reaches
/// out either side.
pub const SHOULDER: f64 = 6.0;
/// The bottom corners' radius at rest, and open.
pub const RADIUS: f64 = 10.0;
pub const RADIUS_OPEN: f64 = 14.0;
/// The curve where the open list meets the notch's row.
pub const STEP: f64 = 8.0;

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
    /// The notch with a slim side either side, its height and no more:
    /// what is drawn at rest, and what the pointer rests on to open it.
    pub fn compact(self) -> Rect {
        let notch = self.frame;
        Rect::new(
            notch.x - SIDE,
            notch.y,
            notch.width + 2.0 * SIDE,
            notch.height,
        )
    }

    /// The panel at rest: the compact notch and its shoulders.
    pub fn rest(self) -> Rect {
        let compact = self.compact();
        Rect::new(
            compact.x - SHOULDER,
            compact.y,
            compact.width + 2.0 * SHOULDER,
            compact.height,
        )
    }

    /// The panel open with `lines` lines under the notch.
    pub fn open(self, lines: usize) -> Rect {
        let rest = self.rest();
        let list = self.list(lines);
        let width = rest.width.max(list.width);
        Rect::new(
            rest.mid_x() - width / 2.0,
            list.y,
            width,
            rest.max_y() - list.y,
        )
    }

    /// The list of `lines` lines, under the notch's row.
    pub fn list(self, lines: usize) -> Rect {
        let compact = self.compact();
        let width = LIST_WIDTH.max(compact.width);
        let height = PAD + count(lines) * LINE + PAD;
        Rect::new(
            compact.mid_x() - width / 2.0,
            compact.y - height,
            width,
            height,
        )
    }

    /// The top of line `line` in the panel, from the panel's top.
    pub fn line_top(self, line: usize) -> f64 {
        self.frame.height + PAD + count(line) * LINE
    }

    /// Which of `lines` lines of the open list is at `point`.
    pub fn line_at(self, lines: usize, point: (f64, f64)) -> Option<usize> {
        let list = self.list(lines);
        if !list.contains(point) {
            return None;
        }
        let below = list.max_y() - PAD - point.1;
        if below < 0.0 {
            return None;
        }
        usize::try_from(round_u64((below / LINE).floor()))
            .ok()
            .filter(|line| *line < lines)
    }

    /// Whether the pointer at `point` is over what is drawn: the compact
    /// notch while closed; it or the list of `lines` lines while open.
    pub fn over(self, open: bool, lines: usize, point: (f64, f64)) -> bool {
        self.compact().contains(point) || (open && self.list(lines).contains(point))
    }

    /// The panel's outline, in the coordinates of a panel at `frame` (y
    /// up from its bottom left): the notch's row with its shoulders, and
    /// open (`lines`), the list under it. At rest and open it is made of
    /// the same parts, so the one can be animated into the other.
    pub fn outline(self, lines: Option<usize>, frame: Rect) -> Vec<Segment> {
        let compact = self.compact();
        let (left, right) = (compact.x - frame.x, compact.max_x() - frame.x);
        let top = compact.max_y() - frame.y;
        let row = compact.height;
        let (half, height, radius) = match lines {
            Some(lines) => {
                let list = self.list(lines);
                (list.width / 2.0, list.height, RADIUS_OPEN)
            }
            None => (compact.width / 2.0, 0.0, RADIUS),
        };
        let middle = f64::midpoint(left, right);
        let (a, b) = (middle - half, middle + half);
        let bottom = top - row - height;
        // Where the list meets the row: at rest only the rounded bottom.
        let meet = (top - row).max(bottom + radius);
        let step = STEP.min(left - a).max(0.0);
        let corner = radius.min(left - a - step).max(0.0);
        let s = SHOULDER;
        vec![
            Segment::Move(left - s, top),
            Segment::Quad((left, top), (left, top - s)),
            Segment::Line(left, meet + step),
            Segment::Quad((left, meet), (left - step, meet)),
            Segment::Line(a + corner, meet),
            Segment::Quad((a, meet), (a, meet - corner)),
            Segment::Line(a, bottom + radius),
            Segment::Quad((a, bottom), (a + radius, bottom)),
            Segment::Line(b - radius, bottom),
            Segment::Quad((b, bottom), (b, bottom + radius)),
            Segment::Line(b, meet - corner),
            Segment::Quad((b, meet), (b - corner, meet)),
            Segment::Line(right + step, meet),
            Segment::Quad((right, meet), (right, meet + step)),
            Segment::Line(right, top - s),
            Segment::Quad((right, top), (right + s, top)),
            Segment::Close,
        ]
    }
}

/// A part of an outline: a move, a line or a curve (its control point,
/// then where it ends) to a point, or the close.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Segment {
    Move(f64, f64),
    Line(f64, f64),
    Quad((f64, f64), (f64, f64)),
    Close,
}

fn count(lines: usize) -> f64 {
    fraction(u64::try_from(lines).unwrap_or(u64::MAX))
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
    fn at_rest_it_is_the_notch_s_height_with_slim_sides_and_shoulders() {
        let notch = find(&[macbook()]).unwrap();
        let compact = notch.compact();
        assert_eq!(compact, Rect::new(633.5, 950.0, 245.0, 32.0));
        assert!((compact.mid_x() - 756.0).abs() < f64::EPSILON, "centred");
        assert_eq!(notch.rest(), Rect::new(627.5, 950.0, 257.0, 32.0));
    }

    #[test]
    fn open_the_list_is_wider_than_the_notch_only_below_its_row() {
        let notch = find(&[macbook()]).unwrap();
        for lines in [1, 3, 9] {
            let list = notch.list(lines);
            assert_eq!((list.x, list.width), (586.0, LIST_WIDTH), "{lines} lines");
            assert!((list.max_y() - 950.0).abs() < f64::EPSILON, "under the row");
            let open = notch.open(lines);
            assert_eq!((open.x, open.width), (586.0, LIST_WIDTH));
            assert!(
                (open.max_y() - 982.0).abs() < f64::EPSILON,
                "flush with the top"
            );
            assert!((open.y - list.y).abs() < f64::EPSILON);
        }
        let open = notch.open(3);
        assert!((open.height - (32.0 + PAD + 3.0 * LINE + PAD)).abs() < f64::EPSILON);
    }

    #[test]
    fn a_point_is_on_the_line_drawn_there() {
        let notch = find(&[macbook()]).unwrap();
        let top = notch.list(3).max_y() - PAD;
        assert_eq!(notch.line_at(3, (756.0, top - 1.0)), Some(0));
        assert_eq!(notch.line_at(3, (756.0, top - LINE - 1.0)), Some(1));
        // Wider than the notch, the list's sides are its lines too.
        assert_eq!(notch.line_at(3, (590.0, top - 1.0)), Some(0));
        // The notch's row, the list's padding, past the last line, or off
        // the list: no line.
        assert_eq!(notch.line_at(3, (756.0, 970.0)), None);
        assert_eq!(notch.line_at(3, (756.0, top + 1.0)), None);
        assert_eq!(notch.line_at(3, (756.0, top - 3.0 * LINE - 1.0)), None);
        assert_eq!(notch.line_at(3, (580.0, top - 1.0)), None);
    }

    #[test]
    fn the_pointer_is_over_the_notch_closed_and_the_list_too_open() {
        let notch = find(&[macbook()]).unwrap();
        let over_notch = (756.0, 970.0);
        let below_notch = (756.0, 930.0);
        let beside_the_row = (600.0, 970.0);
        assert!(notch.over(false, 3, over_notch));
        assert!(!notch.over(false, 3, below_notch), "not open yet");
        assert!(notch.over(true, 3, below_notch), "over the open list");
        assert!(
            !notch.over(true, 3, beside_the_row),
            "the menu bar stays clear"
        );
        assert!(!notch.over(true, 3, (100.0, 100.0)));
    }

    /// The points an outline goes through, ends and control points alike.
    fn points(outline: &[Segment]) -> Vec<(f64, f64)> {
        outline
            .iter()
            .flat_map(|segment| match *segment {
                Segment::Move(x, y) | Segment::Line(x, y) => vec![(x, y)],
                Segment::Quad(control, to) => vec![control, to],
                Segment::Close => vec![],
            })
            .collect()
    }

    #[test]
    fn the_outline_fills_its_panel_and_rest_and_open_are_made_alike() {
        let notch = find(&[macbook()]).unwrap();
        let rest = notch.rest();
        let at_rest = notch.outline(None, rest);
        let inside = |frame: Rect, outline: &[Segment]| {
            points(outline).iter().all(|(x, y)| {
                (-1e-9..=frame.width + 1e-9).contains(x)
                    && (-1e-9..=frame.height + 1e-9).contains(y)
            })
        };
        assert!(inside(rest, &at_rest));
        // At rest: from the screen's top edge at the panel's edges, down to
        // the bottom of the notch's row.
        assert_eq!(at_rest[0], Segment::Move(0.0, 32.0));
        assert_eq!(at_rest[15], Segment::Quad((251.0, 32.0), (257.0, 32.0)));
        let lowest = points(&at_rest)
            .iter()
            .map(|p| p.1)
            .fold(f64::MAX, f64::min);
        assert!(lowest.abs() < f64::EPSILON);

        let open = notch.open(3);
        let opened = notch.outline(Some(3), open);
        assert!(inside(open, &opened));
        assert_eq!(opened.len(), at_rest.len(), "the same parts, to animate");
        // As wide as the list under the row, the row itself as at rest.
        let xs: Vec<f64> = points(&opened).iter().map(|p| p.0).collect();
        assert!(xs.iter().any(|x| x.abs() < f64::EPSILON));
        assert!(xs.iter().any(|x| (x - LIST_WIDTH).abs() < f64::EPSILON));
        let row_left = notch.compact().x - open.x;
        assert_eq!(
            opened[1],
            Segment::Quad((row_left, open.height), (row_left, open.height - SHOULDER))
        );
        // At rest in the open panel's frame: the same shape, moved.
        let moved = notch.outline(None, open);
        assert_eq!(moved.len(), opened.len());
        assert_eq!(moved[0], Segment::Move(row_left - SHOULDER, open.height));
    }
}

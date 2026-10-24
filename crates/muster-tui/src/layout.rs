//! Where the panels are drawn: the person's layout (`muster_core::layout`)
//! of the panels shown, shared out over the window's body. Each panel
//! gets at least what it needs to be read; one of two sized in neither
//! way takes half, but the list, which takes what its lines need. A
//! layout the window is too narrow for is the app's to fold
//! (`App::panels_shown`). Pure: no IO, no app.

use muster_core::layout::{Branch, Node, Panel, Part, Size, Split};
use ratatui::layout::Rect;

/// The fewest columns the list is given, its frame included, however
/// short its names: room for a session's status under its name.
pub(crate) const LIST_LEAST: u16 = 30;
/// The most columns the list takes for its lines, its frame included;
/// longer ones are cut short, and slide.
pub(crate) const LIST_MOST: u16 = 36;

/// The fewest cells each panel is given, its frame included: a session's
/// program wraps what it draws in fewer than about sixty columns, and so
/// would a diff's lines.
const fn least(panel: Panel, list: u16) -> (u16, u16) {
    match panel {
        Panel::List => (list, 6),
        Panel::Session | Panel::Changes | Panel::Pinned(_) => (60, 6),
        Panel::Shell | Panel::Plugin => (40, 6),
    }
}

/// The line between two panels sharing a place, which a drag moves.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Divider {
    /// The split, from the whole window down.
    pub(crate) path: Vec<Branch>,
    pub(crate) split: Split,
    /// The place the two share.
    pub(crate) area: Rect,
    /// The panels on either side of it.
    pub(crate) first: Vec<Panel>,
    pub(crate) second: Vec<Panel>,
    /// Where the second of the two starts: a column side by side, a row
    /// one above the other.
    pub(crate) cut: u16,
}

impl Divider {
    /// Whether a press at `column`, `row` is on the line: side by side,
    /// either frame's edge; one above the other, the bottom edge of the
    /// first, as the top of the second is its title.
    pub(crate) fn holds(&self, column: u16, row: u16) -> bool {
        match self.split {
            Split::Across => {
                (column + 1 == self.cut || column == self.cut)
                    && (self.area.y..self.area.bottom()).contains(&row)
            }
            Split::Down => {
                row + 1 == self.cut && (self.area.x..self.area.right()).contains(&column)
            }
        }
    }

    /// Whether a press at `column`, `row` is near enough to take the line:
    /// on it, or the cell before it, the first's last column or row; never
    /// the second's first column, where a selection of a session's text
    /// starts, nor its first row, its title.
    pub(crate) fn near(&self, column: u16, row: u16) -> bool {
        match self.split {
            Split::Across => {
                (self.cut.saturating_sub(2)..=self.cut).contains(&column)
                    && (self.area.y..self.area.bottom()).contains(&row)
            }
            Split::Down => {
                (self.cut.saturating_sub(2)..self.cut).contains(&row)
                    && (self.area.x..self.area.right()).contains(&column)
            }
        }
    }

    /// The cells the line is drawn in: both frames' edges side by side,
    /// the first's bottom edge one above the other.
    pub(crate) fn line(&self) -> Rect {
        match self.split {
            Split::Across => Rect {
                x: self.cut.saturating_sub(1),
                width: 2,
                ..self.area
            },
            Split::Down => Rect {
                y: self.cut.saturating_sub(1),
                height: 1,
                ..self.area
            },
        }
    }

    /// How far into its place the first of the two reaches: its width side
    /// by side, its height one above the other.
    pub(crate) fn first_extent(&self) -> u16 {
        match self.split {
            Split::Across => self.cut.saturating_sub(self.area.x),
            Split::Down => self.cut.saturating_sub(self.area.y),
        }
    }
}

/// The panels shown, each where it is drawn, and the lines between them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Solved {
    pub(crate) panels: Vec<(Panel, Rect)>,
    pub(crate) dividers: Vec<Divider>,
}

impl Solved {
    /// Where `panel` is drawn; nowhere (an empty area) when not shown.
    pub(crate) fn of(&self, panel: Panel) -> Rect {
        self.panels
            .iter()
            .find(|(shown, _)| *shown == panel)
            .map_or(Rect::default(), |(_, area)| *area)
    }
}

/// The fewest columns and rows `node` is drawn in, the list taking `list`
/// columns.
pub(crate) fn needs(node: &Node, list: u16) -> (u16, u16) {
    let own = |panel: Panel| {
        let (width, height) = least(panel, list);
        match (panel, node.size) {
            (Panel::List, Some(Size::Cells(cells))) => (cells.max(LIST_LEAST), height),
            _ => (width, height),
        }
    };
    match &node.part {
        Part::Panel(panel) => own(*panel),
        Part::Split(split, first, second) => {
            let (first, second) = (needs(first, list), needs(second, list));
            match split {
                Split::Across => (first.0 + second.0, first.1.max(second.1)),
                Split::Down => (first.0.max(second.0), first.1 + second.1),
            }
        }
    }
}

/// `node` laid out over `area`, the list `list` columns wide unless the
/// person gave it a width.
pub(crate) fn solve(node: &Node, area: Rect, list: u16) -> Solved {
    let mut solved = Solved::default();
    place(node, area, list, &mut Vec::new(), &mut solved);
    solved
}

fn place(node: &Node, area: Rect, list: u16, path: &mut Vec<Branch>, solved: &mut Solved) {
    let Part::Split(split, first, second) = &node.part else {
        if let Part::Panel(panel) = node.part {
            solved.panels.push((panel, area));
        }
        return;
    };
    let total = match split {
        Split::Across => area.width,
        Split::Down => area.height,
    };
    let along = |needs: (u16, u16)| match split {
        Split::Across => needs.0,
        Split::Down => needs.1,
    };
    let (first_least, second_least) = (along(needs(first, list)), along(needs(second, list)));
    let wanted = wanted(*split, first, second, total, list);
    let cut = if total >= first_least + second_least {
        wanted.clamp(first_least, total - second_least)
    } else {
        // Short of room: as wanted, each keeping what it can.
        wanted.min(total)
    };
    let (first_area, second_area) = match split {
        Split::Across => (
            Rect { width: cut, ..area },
            Rect {
                x: area.x + cut,
                width: area.width - cut,
                ..area
            },
        ),
        Split::Down => (
            Rect {
                height: cut,
                ..area
            },
            Rect {
                y: area.y + cut,
                height: area.height - cut,
                ..area
            },
        ),
    };
    solved.dividers.push(Divider {
        path: path.clone(),
        split: *split,
        area,
        first: panels(first),
        second: panels(second),
        cut: match split {
            Split::Across => second_area.x,
            Split::Down => second_area.y,
        },
    });
    path.push(Branch::First);
    place(first, first_area, list, path, solved);
    path.pop();
    path.push(Branch::Second);
    place(second, second_area, list, path, solved);
    path.pop();
}

fn panels(node: &Node) -> Vec<Panel> {
    match &node.part {
        Part::Panel(panel) => vec![*panel],
        Part::Split(_, first, second) => {
            let mut all = panels(first);
            all.extend(panels(second));
            all
        }
    }
}

/// The cells the first of two sharing `total` wants: as either was sized,
/// the list's own width beside another, or half.
fn wanted(split: Split, first: &Node, second: &Node, total: u16, list: u16) -> u16 {
    let of = |size: Size| match size {
        Size::Cells(cells) => cells.min(total),
        Size::Share(share) => {
            u16::try_from(u32::from(total) * u32::from(share) / 100).unwrap_or(total)
        }
    };
    let is_list = |node: &Node| split == Split::Across && node.part == Part::Panel(Panel::List);
    if let Some(size) = first.size {
        of(size)
    } else if let Some(size) = second.size {
        total.saturating_sub(of(size))
    } else if is_list(first) {
        list.min(total)
    } else if is_list(second) {
        total.saturating_sub(list)
    } else {
        total / 2
    }
}

#[cfg(test)]
mod tests {
    use muster_core::layout::Layout;

    use super::*;

    const BODY: Rect = Rect {
        x: 0,
        y: 1,
        width: 200,
        height: 40,
    };

    fn solved(text: &str, shown: &[Panel], area: Rect) -> Solved {
        let layout = Layout::parse(text).unwrap();
        let node = layout.shown(|panel| shown.contains(&panel)).unwrap();
        solve(&node, area, 30)
    }

    #[test]
    fn the_list_takes_its_own_width_and_the_session_the_rest() {
        let solved = solved(
            muster_core::layout::DEFAULT,
            &[Panel::List, Panel::Session],
            BODY,
        );
        assert_eq!(solved.of(Panel::List), Rect::new(0, 1, 30, 40));
        assert_eq!(solved.of(Panel::Session), Rect::new(30, 1, 170, 40));
        assert_eq!(solved.of(Panel::Changes), Rect::default(), "not shown");
        assert_eq!(solved.dividers.len(), 1);
        assert!(solved.dividers[0].holds(29, 5) && solved.dividers[0].holds(30, 5));
        assert!(!solved.dividers[0].holds(31, 5));
    }

    #[test]
    fn shares_and_widths_are_as_the_person_gave_them() {
        let solved = solved(
            "list 34 | session 60% | (changes / shell)",
            &[Panel::List, Panel::Session, Panel::Changes, Panel::Shell],
            BODY,
        );
        assert_eq!(solved.of(Panel::List).width, 34);
        // 60% of the 166 the session shares with the column beside it.
        assert_eq!(solved.of(Panel::Session).width, 99);
        let changes = solved.of(Panel::Changes);
        let shell = solved.of(Panel::Shell);
        assert_eq!((changes.x, changes.width, changes.height), (133, 67, 20));
        assert_eq!((shell.y, shell.height), (21, 20));
        // One above the other, the line is the first's bottom edge.
        let down = solved
            .dividers
            .iter()
            .find(|divider| divider.split == Split::Down)
            .unwrap();
        assert_eq!(down.path, vec![Branch::Second, Branch::Second]);
        assert!(down.holds(140, 20) && !down.holds(140, 21));
    }

    #[test]
    fn a_share_gives_way_to_what_a_panel_needs() {
        // 10% would leave the session fewer columns than it is read in.
        let solved = solved(
            "list | session 10% | (changes / shell)",
            &[Panel::List, Panel::Session, Panel::Changes],
            Rect::new(0, 1, 160, 30),
        );
        assert_eq!(solved.of(Panel::Session).width, 60);
        assert_eq!(solved.of(Panel::Changes).width, 70);
    }

    #[test]
    fn the_list_on_the_right_keeps_its_width() {
        let solved = solved(
            "session | (changes / shell) | list",
            &[Panel::List, Panel::Session],
            Rect::new(0, 1, 120, 30),
        );
        assert_eq!(solved.of(Panel::List), Rect::new(90, 1, 30, 30));
        assert_eq!(solved.of(Panel::Session), Rect::new(0, 1, 90, 30));
    }

    #[test]
    fn what_a_layout_needs_adds_up_side_by_side_and_one_above_the_other() {
        let layout = Layout::default();
        let all = layout.shown(|_| true).unwrap();
        // The list's 30, the session's 60, and the changes' 60 above the
        // shell's 40 above a plugin's panel's 40.
        assert_eq!(needs(&all, 30), (150, 18));
        let two = layout
            .shown(|panel| matches!(panel, Panel::List | Panel::Session))
            .unwrap();
        assert_eq!(needs(&two, 30), (90, 6));
    }
}

//! The list under the notch while it is open: each session on two lines,
//! its agent's logo, its name and how long, then its project and what it
//! is doing; a rule between those waiting on the person and those
//! working; the line the pointer is on lit; how many more there are, and
//! a note.

use objc2::rc::Retained;
use objc2_app_kit::{NSFont, NSLineBreakMode, NSView};
use objc2_foundation::MainThreadMarker;

use super::look::{self, Colour, Label, Logo};
use super::ns_rect;
use crate::board::{Board, MOST_ROWS, Mark, Row};
use crate::geometry::{LINE, Notch, Rect};

/// Room between the list's edges and what is in it.
const INSET: f64 = 12.0;
/// A line of text's height.
const TEXT: f64 = 16.0;
/// The most a project's name takes of a line.
const PROJECT: f64 = 110.0;

/// One listed session's parts.
struct Line {
    logo: Logo,
    mark: Label,
    title: Label,
    since: Label,
    dot: Retained<NSView>,
    project: Label,
    doing: Label,
}

/// The list's parts, made once and shown as the board says.
pub(super) struct List {
    lit: Retained<NSView>,
    rule: Retained<NSView>,
    lines: Vec<Line>,
    more: Label,
    note: Label,
}

/// What the list draws now.
pub(super) struct Drawn<'a> {
    pub(super) board: &'a Board,
    pub(super) notch: Notch,
    /// The panel, in screen coordinates.
    pub(super) frame: Rect,
    /// The lines the list is drawn for, its rows among them.
    pub(super) lines: usize,
    /// The line the pointer is on.
    pub(super) pointed: Option<usize>,
    pub(super) note: Option<&'a str>,
}

impl List {
    pub(super) fn new(mtm: MainThreadMarker, view: &NSView) -> Self {
        // Made first, so it is under the words.
        let lit = look::shape(mtm, view, Colour::Tint, 7.0);
        let rule = look::shape(mtm, view, Colour::Line, 0.0);
        let text = NSFont::systemFontOfSize(12.0);
        let title = NSFont::systemFontOfSize_weight(12.0, 0.23);
        let small = NSFont::systemFontOfSize(11.0);
        let digits = NSFont::monospacedDigitSystemFontOfSize_weight(11.0, 0.0);
        let lines = (0..MOST_ROWS)
            .map(|_| {
                let doing = Label::new(mtm, view, &small, false);
                // A path keeps its file's name, cut short in the middle.
                doing
                    .field
                    .setLineBreakMode(NSLineBreakMode::ByTruncatingMiddle);
                Line {
                    logo: Logo::new(mtm, view),
                    mark: Label::new(mtm, view, &text, false),
                    title: Label::new(mtm, view, &title, false),
                    since: Label::new(mtm, view, &digits, true),
                    dot: look::shape(mtm, view, Colour::Quiet, 3.0),
                    project: Label::new(mtm, view, &small, false),
                    doing,
                }
            })
            .collect();
        let more = Label::new(mtm, view, &small, false);
        let note = Label::new(mtm, view, &small, false);
        // Two lines, so a reason fits the list's width.
        note.field.setLineBreakMode(NSLineBreakMode::ByWordWrapping);
        note.field.setMaximumNumberOfLines(2);
        Self {
            lit,
            rule,
            lines,
            more,
            note,
        }
    }

    /// Draws `drawn`, or nothing where `drawn` is `None`: the list closed.
    pub(super) fn draw(&mut self, drawn: Option<Drawn<'_>>) {
        let Some(drawn) = drawn else {
            self.hide();
            return;
        };
        let Drawn {
            board,
            notch,
            frame,
            lines,
            pointed,
            note,
        } = drawn;
        let list = notch.list(lines);
        let (left, width) = (list.x - frame.x + INSET, list.width - 2.0 * INSET);
        let right = left + width;
        // A line's top, and its first and second rows of text (y up).
        let top = |line: usize| frame.height - notch.line_top(line);
        let first = |line: usize| top(line) - 2.0 - TEXT;
        let second = |line: usize| first(line) - 15.0;

        match pointed.filter(|line| *line < board.rows.len()) {
            Some(line) => {
                let lit = Rect::new(left - 6.0, top(line) - LINE + 1.0, width + 12.0, LINE - 2.0);
                self.lit.setFrame(ns_rect(lit));
                self.lit.setHidden(false);
            }
            None => self.lit.setHidden(true),
        }
        match board.first_working() {
            Some(line) => {
                self.rule
                    .setFrame(ns_rect(Rect::new(left, top(line), width, 1.0)));
                self.rule.setHidden(false);
            }
            None => self.rule.setHidden(true),
        }

        for (at, line) in self.lines.iter_mut().enumerate() {
            match board.rows.get(at) {
                Some(row) => line.draw(row, left, right, first(at)),
                None => line.hide(),
            }
        }

        let mut next = board.rows.len();
        if board.more > 0 {
            self.more.show(
                &format!("and {} more in Muster", board.more),
                Colour::Quiet,
                Rect::new(left + 20.0, first(next), width - 20.0, TEXT),
            );
            next += 1;
        } else {
            self.more.hide();
        }
        match note {
            Some(text) => self.note.show(
                text,
                Colour::Working,
                Rect::new(left, second(next), width, 2.0 * TEXT),
            ),
            None => self.note.hide(),
        }
    }

    fn hide(&mut self) {
        self.lit.setHidden(true);
        self.rule.setHidden(true);
        for line in &mut self.lines {
            line.hide();
        }
        self.more.hide();
        self.note.hide();
    }
}

impl Line {
    /// Draws `row` between `left` and `right`, its first row of text at
    /// `first` and its second under it.
    fn draw(&mut self, row: &Row, left: f64, right: f64, first: f64) {
        let second = first - 15.0;
        let (mark, colour, status) = match row.mark {
            Mark::Asking => ("▲", Colour::Asking, "Asking you"),
            Mark::News => ("✓", Colour::News, "Finished"),
            Mark::Working => ("●", Colour::Working, "Working"),
        };
        if let Some(png) = look::logo(&row.harness) {
            self.logo
                .show(Some(png), Rect::new(left, first + 1.0, 14.0, 14.0));
            self.mark.hide();
        } else {
            self.logo.hide();
            self.mark
                .show(mark, colour, Rect::new(left, first, 16.0, TEXT));
        }
        let quiet_time = row.mark == Mark::Working;
        let since = self.since.fit(
            &row.since,
            if quiet_time { Colour::Quiet } else { colour },
            right,
            first,
            TEXT,
        );
        self.title.show(
            &row.title,
            Colour::Text,
            Rect::new(left + 20.0, first, right - left - 20.0 - since - 8.0, TEXT),
        );
        self.title.say(&format!(
            "{status}: {}, in {}, {}, {}",
            row.title, row.project, row.doing, row.since
        ));
        if let Some(layer) = self.dot.layer() {
            layer.setBackgroundColor(Some(&Colour::Project(row.colour).ns().CGColor()));
        }
        self.dot
            .setFrame(ns_rect(Rect::new(left + 21.0, second + 5.0, 6.0, 6.0)));
        self.dot.setHidden(false);
        // The project as wide as its name, up to `PROJECT`.
        let at = |width: f64| Rect::new(left + 31.0, second, width, TEXT);
        self.project.show(&row.project, Colour::Quiet, at(PROJECT));
        let project = self.project.width().min(PROJECT);
        self.project.show(&row.project, Colour::Quiet, at(project));
        let doing = left + 31.0 + project + 8.0;
        self.doing.show(
            &row.doing,
            Colour::Quiet,
            Rect::new(doing, second, right - doing, TEXT),
        );
    }

    fn hide(&self) {
        self.logo.hide();
        for label in [
            &self.mark,
            &self.title,
            &self.since,
            &self.project,
            &self.doing,
        ] {
            label.hide();
        }
        self.dot.setHidden(true);
    }
}

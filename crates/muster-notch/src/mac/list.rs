//! The list under the notch while it is open: each session on two lines,
//! its agent's logo, its name and how long, then its project and what it
//! is doing; a rule between those waiting on the person and those
//! working; the line the pointer is on lit; how many more there are, and
//! a note. Each line is a view of its own, so the lines can come in one
//! after another as the list opens.

use objc2::rc::Retained;
use objc2_app_kit::{NSFont, NSLineBreakMode, NSView};
use objc2_foundation::MainThreadMarker;

use super::look::{self, Colour, Label, Logo};
use super::{ns_rect, shape};
use crate::board::{Board, MOST_ROWS, Mark, Row};
use crate::geometry::{LINE, Notch, Rect};

/// Room between the list's edges and what is in it.
const INSET: f64 = 18.0;
/// A line of text's height.
const TEXT: f64 = 16.0;
/// Room above a line's first row of text.
const ABOVE: f64 = 6.0;
/// From a line's first row of text to its second.
const UNDER: f64 = 17.0;
/// The most a project's name takes of a line.
const PROJECT: f64 = 120.0;
/// When the first line comes in after the list starts to open, and how
/// much later each next one does, in seconds.
const FIRST_IN: f64 = 0.06;
const EACH_IN: f64 = 0.025;

/// One listed session's parts, in a view of its own.
struct Line {
    view: Retained<NSView>,
    logo: Logo,
    mark: Label,
    title: Label,
    since: Label,
    dot: Retained<NSView>,
    project: Label,
    doing: Label,
    /// What its work comes to, at the second row's end.
    work: Label,
}

/// The list's parts, made once and shown as the board says.
pub(super) struct List {
    content: Retained<NSView>,
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
        let content = NSView::new(mtm);
        content.setWantsLayer(true);
        content.setAlphaValue(0.0);
        view.addSubview(&content);
        // Made first, so it is under the words.
        let lit = look::shape(mtm, &content, Colour::Tint, 9.0);
        let rule = look::shape(mtm, &content, Colour::Line, 0.0);
        let text = NSFont::systemFontOfSize(12.0);
        let title = NSFont::systemFontOfSize_weight(12.5, 0.23);
        let small = NSFont::systemFontOfSize(11.0);
        let digits = NSFont::monospacedDigitSystemFontOfSize_weight(11.0, 0.0);
        let lines = (0..MOST_ROWS)
            .map(|_| {
                let view = NSView::new(mtm);
                view.setWantsLayer(true);
                content.addSubview(&view);
                let doing = Label::new(mtm, &view, &small, false);
                // A path keeps its file's name, cut short in the middle.
                doing
                    .field
                    .setLineBreakMode(NSLineBreakMode::ByTruncatingMiddle);
                Line {
                    logo: Logo::new(mtm, &view),
                    mark: Label::new(mtm, &view, &text, false),
                    title: Label::new(mtm, &view, &title, false),
                    since: Label::new(mtm, &view, &digits, true),
                    dot: look::shape(mtm, &view, Colour::Quiet, 3.0),
                    project: Label::new(mtm, &view, &small, false),
                    doing,
                    work: Label::new(mtm, &view, &digits, true),
                    view,
                }
            })
            .collect();
        let more = Label::new(mtm, &content, &small, false);
        let note = Label::new(mtm, &content, &small, false);
        // Two lines, so a reason fits the list's width.
        note.field.setLineBreakMode(NSLineBreakMode::ByWordWrapping);
        note.field.setMaximumNumberOfLines(2);
        Self {
            content,
            lit,
            rule,
            lines,
            more,
            note,
        }
    }

    /// The list comes in: each of its lines a little after the one above.
    pub(super) fn reveal(&self) {
        self.content.setAlphaValue(1.0);
        if let Some(layer) = self.content.layer() {
            layer.removeAllAnimations();
        }
        let mut after = FIRST_IN;
        for line in &self.lines {
            shape::appear(&line.view, after);
            after += EACH_IN;
        }
        shape::appear(&self.rule, FIRST_IN);
    }

    /// The list goes, quickly, as the panel closes.
    pub(super) fn conceal(&self) {
        shape::disappear(&self.content);
    }

    /// Draws `drawn`.
    pub(super) fn draw(&mut self, drawn: &Drawn<'_>) {
        let &Drawn {
            board,
            notch,
            frame,
            lines,
            pointed,
            note,
        } = drawn;
        self.content
            .setFrame(ns_rect(Rect::new(0.0, 0.0, frame.width, frame.height)));
        let list = notch.list(lines);
        let (left, width) = (list.x - frame.x + INSET, list.width - 2.0 * INSET);
        // A line's top in the panel (y up), and where its box goes.
        let top = |line: usize| frame.height - notch.line_top(line);
        let place = |line: usize| Rect::new(left, top(line) - LINE, width, LINE);

        match pointed.filter(|line| *line < board.rows.len()) {
            Some(line) => {
                let lit = place(line);
                let lit = Rect::new(lit.x - 8.0, lit.y + 1.0, lit.width + 16.0, lit.height - 2.0);
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
                Some(row) => line.draw(row, place(at)),
                None => line.view.setHidden(true),
            }
        }
        // After the rows, in a line's first row of text.
        let first = |line: usize| top(line) - ABOVE - TEXT;
        let mut next = board.rows.len();
        if board.more > 0 {
            self.more.show(
                &format!("and {} more in Muster", board.more),
                Colour::Quiet,
                Rect::new(left + 24.0, first(next), width - 24.0, TEXT),
            );
            next += 1;
        } else {
            self.more.hide();
        }
        match note {
            Some(text) => self.note.show(
                text,
                Colour::Working,
                Rect::new(left, first(next) - TEXT, width, 2.0 * TEXT),
            ),
            None => self.note.hide(),
        }
    }
}

impl Line {
    /// Draws `row` in its box `place`, in the panel's coordinates.
    fn draw(&mut self, row: &Row, place: Rect) {
        self.view.setFrame(ns_rect(place));
        self.view.setHidden(false);
        // In the line's own coordinates (y up).
        let right = place.width;
        let first = place.height - ABOVE - TEXT;
        let second = first - UNDER;
        let (mark, colour, status) = match row.mark {
            Mark::Asking => ("▲", Colour::Asking, "Asking you"),
            Mark::News => ("✓", Colour::News, "Finished"),
            Mark::Working => ("●", Colour::Working, "Working"),
            Mark::Conflict => ("⇄", Colour::Asking, "Conflict"),
            Mark::ChecksFailed => ("✗", Colour::Asking, "Checks failed"),
        };
        if let Some(png) = look::logo(&row.harness) {
            self.logo
                .show(Some(png), Rect::new(0.0, first + 0.5, 15.0, 15.0));
            self.mark.hide();
        } else {
            self.logo.hide();
            self.mark
                .show(mark, colour, Rect::new(0.0, first, 16.0, TEXT));
        }
        let quiet_time = row.mark == Mark::Working;
        let since = self.since.fit(
            &row.since,
            if quiet_time { Colour::Quiet } else { colour },
            right,
            first,
            TEXT,
        );
        let text = 24.0;
        self.title.show(
            &row.title,
            Colour::Text,
            Rect::new(text, first, right - text - since - 10.0, TEXT),
        );
        self.title.say(&format!(
            "{status}: {}, in {}, {}, {} {}",
            row.title, row.project, row.doing, row.work, row.since
        ));
        if let Some(layer) = self.dot.layer() {
            layer.setBackgroundColor(Some(&Colour::Project(row.colour).ns().CGColor()));
        }
        self.dot
            .setFrame(ns_rect(Rect::new(text + 1.0, second + 5.0, 6.0, 6.0)));
        self.dot.setHidden(false);
        // The project as wide as its name, up to `PROJECT`.
        let at = |width: f64| Rect::new(text + 11.0, second, width, TEXT);
        self.project.show(&row.project, Colour::Quiet, at(PROJECT));
        let project = self.project.width().min(PROJECT);
        self.project.show(&row.project, Colour::Quiet, at(project));
        // What its work comes to at the end, what it is doing between.
        let work = if row.work.is_empty() {
            self.work.hide();
            0.0
        } else {
            self.work.fit(&row.work, Colour::Quiet, right, second, TEXT) + 10.0
        };
        let doing = text + 11.0 + project + 10.0;
        self.doing.show(
            &row.doing,
            Colour::Quiet,
            Rect::new(doing, second, right - work - doing, TEXT),
        );
    }
}

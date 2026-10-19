//! How the notch draws its words and marks: Muster's own colours, as the
//! UI draws them on a dark terminal, on the notch's black; a line of text
//! that changes only when what it says does; and each agent's logo.

use objc2::AnyThread;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSAccessibility, NSColor, NSFont, NSImage, NSImageView, NSLineBreakMode, NSTextAlignment,
    NSTextField, NSView,
};
use objc2_foundation::{MainThreadMarker, NSData, NSString};

use super::ns_rect;
use crate::geometry::Rect;

/// The colours the notch draws in: the UI's own on a dark terminal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Colour {
    Text,
    Quiet,
    /// The rule between the list's groups.
    Line,
    /// Behind the line the pointer is on.
    Tint,
    Working,
    Asking,
    News,
    Project(usize),
}

impl Colour {
    pub(super) fn ns(self) -> Retained<NSColor> {
        // The UI's projects' colours, in the order they are listed.
        const PROJECTS: [u32; 6] = [
            0x8c_b3f7, 0xc7_a2f4, 0x86_d6cc, 0xf0_b3d8, 0xa7_b5f7, 0x7f_c5e6,
        ];
        let rgb = match self {
            Self::Text => 0xd8_dae5,
            Self::Quiet => 0x8e_93a8,
            Self::Line => 0x3e_4154,
            Self::Tint => 0x2a_2b3a,
            Self::Working => 0xf2_cc8f,
            Self::Asking => 0xf2_8b9d,
            Self::News => 0x8a_d1e8,
            Self::Project(at) => PROJECTS[at % PROJECTS.len()],
        };
        let part = |shift: u32| f64::from((rgb >> shift) & 0xff) / 255.0;
        NSColor::colorWithSRGBRed_green_blue_alpha(part(16), part(8), part(0), 1.0)
    }
}

/// A line of text in the panel, changed only when what it says changes.
pub(super) struct Label {
    pub(super) field: Retained<NSTextField>,
    text: String,
    colour: Colour,
}

impl Label {
    pub(super) fn new(mtm: MainThreadMarker, view: &NSView, font: &NSFont, right: bool) -> Self {
        let field = NSTextField::labelWithString(&NSString::from_str(""), mtm);
        field.setFont(Some(font));
        field.setTextColor(Some(&Colour::Text.ns()));
        field.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
        if right {
            field.setAlignment(NSTextAlignment::Right);
        }
        field.setHidden(true);
        view.addSubview(&field);
        Self {
            field,
            text: String::new(),
            colour: Colour::Text,
        }
    }

    /// Shows `text` in `colour` at `frame`, in the view's coordinates.
    pub(super) fn show(&mut self, text: &str, colour: Colour, frame: Rect) {
        if self.text != text {
            text.clone_into(&mut self.text);
            self.field.setStringValue(&NSString::from_str(text));
        }
        if self.colour != colour {
            self.colour = colour;
            self.field.setTextColor(Some(&colour.ns()));
        }
        self.field.setFrame(ns_rect(frame));
        self.field.setHidden(false);
    }

    pub(super) fn hide(&self) {
        self.field.setHidden(true);
    }

    /// How wide its text is drawn.
    pub(super) fn width(&self) -> f64 {
        self.field.sizeToFit();
        self.field.frame().size.width
    }

    /// Shows `text` as wide as it is drawn, ending at `right`; returns
    /// that width.
    pub(super) fn fit(
        &mut self,
        text: &str,
        colour: Colour,
        right: f64,
        y: f64,
        height: f64,
    ) -> f64 {
        self.show(text, colour, Rect::new(0.0, y, 1.0, height));
        let width = self.width();
        self.show(text, colour, Rect::new(right - width, y, width, height));
        width
    }

    /// Shows `text` as wide as it is drawn, centred on `middle`.
    pub(super) fn centre(&mut self, text: &str, colour: Colour, middle: f64, y: f64, height: f64) {
        self.show(text, colour, Rect::new(0.0, y, 1.0, height));
        let width = self.width();
        self.show(
            text,
            colour,
            Rect::new(middle - width / 2.0, y, width, height),
        );
    }

    /// What a screen reader says of it, in place of what it shows.
    pub(super) fn say(&self, words: &str) {
        self.field
            .setAccessibilityLabel(Some(&NSString::from_str(words)));
    }
}

/// A small mark drawn as a shape: a dot, or a rule.
pub(super) fn shape(
    mtm: MainThreadMarker,
    view: &NSView,
    colour: Colour,
    radius: f64,
) -> Retained<NSView> {
    let shape = NSView::new(mtm);
    shape.setWantsLayer(true);
    if let Some(layer) = shape.layer() {
        layer.setBackgroundColor(Some(&colour.ns().CGColor()));
        layer.setCornerRadius(radius);
    }
    shape.setHidden(true);
    view.addSubview(&shape);
    shape
}

/// The logo of the agent the harness `harness` runs, as the UI draws it;
/// the review is Claude Code's.
pub(super) fn logo(harness: &str) -> Option<&'static [u8]> {
    Some(match harness {
        "claude" | "review" => include_bytes!("../../../muster-tui/assets/logos/claude.png"),
        "codex" => include_bytes!("../../../muster-tui/assets/logos/codex.png"),
        "gemini" => include_bytes!("../../../muster-tui/assets/logos/gemini.png"),
        "opencode" => include_bytes!("../../../muster-tui/assets/logos/opencode.png"),
        "copilot" => include_bytes!("../../../muster-tui/assets/logos/copilot.png"),
        _ => return None,
    })
}

/// A listed session's logo, the picture changed only when its agent does.
pub(super) struct Logo {
    pub(super) view: Retained<NSImageView>,
    shown: Option<&'static [u8]>,
}

impl Logo {
    pub(super) fn new(mtm: MainThreadMarker, view: &NSView) -> Self {
        let image = NSImageView::new(mtm);
        image.setHidden(true);
        view.addSubview(&image);
        Self {
            view: image,
            shown: None,
        }
    }

    /// Shows the picture `png` at `frame`; hidden where there is none.
    pub(super) fn show(&mut self, png: Option<&'static [u8]>, frame: Rect) {
        let Some(png) = png else {
            self.view.setHidden(true);
            return;
        };
        if self.shown.is_none_or(|shown| !std::ptr::eq(shown, png)) {
            let data = NSData::with_bytes(png);
            let image = NSImage::initWithData(NSImage::alloc(), &data);
            self.view.setImage(image.as_deref());
            self.shown = Some(png);
        }
        self.view.setFrame(ns_rect(frame));
        self.view.setHidden(false);
    }

    pub(super) fn hide(&self) {
        self.view.setHidden(true);
    }
}

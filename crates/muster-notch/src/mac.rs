//! The notch on screen, through AppKit (`ARC-22`): a borderless panel that
//! never takes focus, above the menu bar on every Space, flush with the top
//! of the screen and no wider than the notch, black like it. At rest it is
//! the notch and a strip under it, Kep's face and the counts in the strip;
//! pointed at, it opens downwards into the list.
//!
//! AppKit is used from the main thread only: the link to the daemon runs on
//! a thread of its own and hands each board over on the main queue. The
//! only `unsafe` code in the crate is here (`ARC-12`): making the panel,
//! and the view class whose methods AppKit calls, each with its reasoning
//! beside it.

#![expect(
    unsafe_code,
    reason = "ARC-12: AppKit's panel and a view class of the notch's own"
)]

use std::cell::RefCell;
use std::path::PathBuf;

use dispatch2::DispatchQueue;
use muster_core::SessionId;
use objc2::rc::Retained;
use objc2::{MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSColor, NSEvent, NSFont,
    NSLineBreakMode, NSPanel, NSResponder, NSScreen, NSStatusWindowLevel, NSTextAlignment,
    NSTextField, NSView, NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_foundation::{MainThreadMarker, NSObject, NSPoint, NSRect, NSSize, NSString};
use objc2_quartz_core::CACornerMask;
use tokio::sync::mpsc;

use crate::board::{Board, MOST_ROWS, Mark, Mood};
use crate::geometry::{self, BAND, Notch, Rect};
use crate::link::{self, Ended, ToScreen};
use crate::{MOVED_FOR, NotchError};

/// How long a note stays under the list, in steps: three seconds.
const NOTE_STEPS: u64 = 30;

thread_local! {
    /// The notch on screen; on the main thread only.
    static SCREEN: RefCell<Option<Screen>> = const { RefCell::new(None) };
}

/// Shows the notch until the link ends, on this thread, which must be the
/// program's main thread.
pub(crate) fn run(
    socket: PathBuf,
    relaunch: impl FnOnce(&str) -> std::io::Error + Send + 'static,
) -> Result<(), NotchError> {
    let mtm = MainThreadMarker::new().ok_or(NotchError::NotMainThread)?;
    if geometry::find(&screens(mtm)).is_none() {
        // No screen here has a notch.
        return Ok(());
    }
    let app = NSApplication::sharedApplication(mtm);
    // No Dock icon and no menu bar of its own: the notch is all it shows.
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    let (picks, picked) = mpsc::channel(8);
    SCREEN.with(|screen| *screen.borrow_mut() = Some(Screen::new(mtm, picks)));
    // Lives as long as the notch: when the link ends, the program does.
    std::thread::Builder::new()
        .name("muster-notch-link".to_owned())
        .spawn(move || follow(&socket, picked, relaunch))?;
    app.run();
    Ok(())
}

/// Follows the daemon on this thread, handing the screen each board, until
/// the notch should end; then ends the program, or moves it onto the one
/// installed now.
fn follow(
    socket: &std::path::Path,
    picked: mpsc::Receiver<SessionId>,
    relaunch: impl FnOnce(&str) -> std::io::Error,
) {
    let ended = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => {
            let moved_for = std::env::var(MOVED_FOR).ok();
            let same_version = move |daemon: &str| {
                daemon == env!("CARGO_PKG_VERSION") || moved_for.as_deref() == Some(daemon)
            };
            runtime.block_on(link::run(socket, picked, same_version, |update| {
                DispatchQueue::main().exec_async(move || on_main(update));
            }))
        }
        Err(error) => Ended::NoDaemon(error.to_string()),
    };
    if let Ended::NewVersion(version) = &ended {
        let error = relaunch(version);
        eprintln!("muster notch: could not move onto the program installed now: {error}");
    }
    if let Ended::NoDaemon(why) = &ended {
        eprintln!("muster notch: {why}");
    }
    std::process::exit(0);
}

/// Hands an update to the screen; runs on the main queue.
fn on_main(update: ToScreen) {
    SCREEN.with(|screen| {
        if let Some(screen) = screen.borrow_mut().as_mut() {
            screen.update(update);
        }
    });
}

/// What AppKit says of the screens now.
fn screens(mtm: MainThreadMarker) -> Vec<geometry::Screen> {
    NSScreen::screens(mtm)
        .iter()
        .map(|screen| {
            let insets = screen.safeAreaInsets();
            geometry::Screen {
                frame: rect_of(screen.frame()),
                safe_top: insets.top,
                left: rect_of(screen.auxiliaryTopLeftArea()),
                right: rect_of(screen.auxiliaryTopRightArea()),
            }
        })
        .collect()
}

fn rect_of(rect: NSRect) -> Rect {
    Rect::new(
        rect.origin.x,
        rect.origin.y,
        rect.size.width,
        rect.size.height,
    )
}

fn ns_rect(rect: Rect) -> NSRect {
    NSRect::new(
        NSPoint::new(rect.x, rect.y),
        NSSize::new(rect.width, rect.height),
    )
}

define_class!(
    // SAFETY: NSView asks nothing more of a subclass than that its methods
    // keep AppKit's signatures, as these do, and this class has no `Drop`.
    #[unsafe(super(NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "MusterNotchView"]
    struct NotchView;

    impl NotchView {
        // SAFETY: NSView's `- (BOOL)acceptsFirstMouse:(NSEvent *)event`; the
        // notch's application is never active, so without it the first
        // click would only activate it.
        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
            true
        }

        // SAFETY: NSResponder's `- (void)mouseDown:(NSEvent *)event`.
        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, _event: &NSEvent) {
            let at = NSEvent::mouseLocation();
            // Not borrowed now: AppKit calls this between updates.
            SCREEN.with(|screen| {
                if let Ok(mut screen) = screen.try_borrow_mut()
                    && let Some(screen) = screen.as_mut()
                {
                    screen.clicked((at.x, at.y));
                }
            });
        }
    }
);

impl NotchView {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(());
        // SAFETY: NSView's designated `init`, which takes nothing more.
        unsafe { msg_send![super(this), init] }
    }
}

/// The colours the notch draws in, as the UI's terminal colours read on
/// black.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Colour {
    White,
    Dim,
    Working,
    Asking,
    News,
    Project(usize),
}

impl Colour {
    fn ns(self) -> Retained<NSColor> {
        // Blue, magenta and cyan, then their light forms, as the UI's.
        const PROJECTS: [(f64, f64, f64); 6] = [
            (0.38, 0.62, 1.0),
            (0.86, 0.47, 0.95),
            (0.35, 0.84, 0.92),
            (0.62, 0.78, 1.0),
            (0.95, 0.66, 1.0),
            (0.62, 0.95, 1.0),
        ];
        let (red, green, blue) = match self {
            Self::White => (1.0, 1.0, 1.0),
            Self::Dim => (0.56, 0.56, 0.58),
            Self::Working => (1.0, 0.8, 0.26),
            Self::Asking => (1.0, 0.36, 0.36),
            Self::News => (0.42, 0.88, 0.5),
            Self::Project(at) => PROJECTS[at % PROJECTS.len()],
        };
        NSColor::colorWithSRGBRed_green_blue_alpha(red, green, blue, 1.0)
    }
}

/// A line of text in the panel, changed only when what it says changes.
struct Label {
    field: Retained<NSTextField>,
    text: String,
    colour: Colour,
}

impl Label {
    fn new(mtm: MainThreadMarker, view: &NSView, font: &NSFont, right: bool) -> Self {
        let field = NSTextField::labelWithString(&NSString::from_str(""), mtm);
        field.setFont(Some(font));
        field.setTextColor(Some(&Colour::White.ns()));
        field.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
        if right {
            field.setAlignment(NSTextAlignment::Right);
        }
        field.setHidden(true);
        view.addSubview(&field);
        Self {
            field,
            text: String::new(),
            colour: Colour::White,
        }
    }

    /// Shows `text` in `colour` at `frame`, in the view's coordinates.
    fn show(&mut self, text: &str, colour: Colour, frame: Rect) {
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

    fn hide(&self) {
        self.field.setHidden(true);
    }

    /// How wide its text is drawn.
    fn width(&self) -> f64 {
        self.field.sizeToFit();
        self.field.frame().size.width
    }

    /// Shows `text` as wide as it is drawn, ending at `right`; returns
    /// that width.
    fn fit(&mut self, text: &str, colour: Colour, right: f64, y: f64, height: f64) -> f64 {
        self.show(text, colour, Rect::new(0.0, y, 1.0, height));
        let width = self.width();
        self.show(text, colour, Rect::new(right - width, y, width, height));
        width
    }

    fn fade(&self, alpha: f64) {
        self.field.setAlphaValue(alpha);
    }
}

/// One listed session's labels.
struct Line {
    mark: Label,
    project: Label,
    title: Label,
    doing: Label,
    since: Label,
}

/// The notch on screen.
struct Screen {
    mtm: MainThreadMarker,
    panel: Retained<NSPanel>,
    view: Retained<NotchView>,
    face: Label,
    waiting: Label,
    working: Label,
    lines: Vec<Line>,
    more: Label,
    note: Label,
    board: Board,
    step: u64,
    /// A note under the list, and the step it came at.
    said: Option<(String, u64)>,
    /// Whether the list is open, pointed at.
    open: bool,
    /// Whether the panel is on screen.
    visible: bool,
    /// Where the notch was when last drawn.
    notch: Option<Notch>,
    /// Where the sessions the person picks go.
    picks: mpsc::Sender<SessionId>,
}

impl Screen {
    fn new(mtm: MainThreadMarker, picks: mpsc::Sender<SessionId>) -> Self {
        // Kept (`Retained`) for as long as the program runs, and never
        // closed.
        let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
            NSPanel::alloc(mtm),
            NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(1.0, 1.0)),
            NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel,
            NSBackingStoreType::Buffered,
            false,
        );
        // SAFETY: a window AppKit would release on closing must not be; the
        // panel is held above, for as long as the program runs.
        unsafe { panel.setReleasedWhenClosed(false) };
        panel.setFloatingPanel(true);
        // Above the menu bar and its status items, as the notch is.
        panel.setLevel(NSStatusWindowLevel + 2);
        panel.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::Stationary
                | NSWindowCollectionBehavior::FullScreenAuxiliary
                | NSWindowCollectionBehavior::IgnoresCycle,
        );
        panel.setOpaque(false);
        panel.setBackgroundColor(Some(&NSColor::clearColor()));
        panel.setHasShadow(false);
        panel.setMovable(false);
        panel.setHidesOnDeactivate(false);

        let view = NotchView::new(mtm);
        view.setWantsLayer(true);
        if let Some(layer) = view.layer() {
            layer.setBackgroundColor(Some(&NSColor::blackColor().CGColor()));
            // The bottom corners rounded; the top meets the screen's edge.
            layer.setMaskedCorners(
                CACornerMask::LayerMinXMinYCorner | CACornerMask::LayerMaxXMinYCorner,
            );
        }
        panel.setContentView(Some(&view));

        let strip = NSFont::monospacedDigitSystemFontOfSize_weight(12.0, 0.3);
        let text = NSFont::systemFontOfSize(12.0);
        let small = NSFont::systemFontOfSize(11.0);
        let small_bold = NSFont::boldSystemFontOfSize(11.0);
        let face = Label::new(mtm, &view, &strip, false);
        let waiting = Label::new(mtm, &view, &strip, false);
        let working = Label::new(mtm, &view, &strip, true);
        let lines = (0..MOST_ROWS)
            .map(|_| Line {
                mark: Label::new(mtm, &view, &text, false),
                title: Label::new(mtm, &view, &text, false),
                since: Label::new(mtm, &view, &small, true),
                project: Label::new(mtm, &view, &small_bold, false),
                doing: Label::new(mtm, &view, &small, false),
            })
            .collect();
        let more = Label::new(mtm, &view, &small, false);
        let note = Label::new(mtm, &view, &small, false);
        // Two lines, so a reason fits as narrow as the notch.
        note.field.setLineBreakMode(NSLineBreakMode::ByWordWrapping);
        note.field.setMaximumNumberOfLines(2);
        Self {
            mtm,
            panel,
            view,
            face,
            waiting,
            working,
            lines,
            more,
            note,
            board: Board::hidden(),
            step: 0,
            said: None,
            open: false,
            visible: false,
            notch: None,
            picks,
        }
    }

    fn update(&mut self, update: ToScreen) {
        match update {
            ToScreen::Board { board, step } => {
                self.board = board;
                self.step = step;
            }
            ToScreen::Note(text) => self.said = Some((text, self.step)),
        }
        self.draw();
    }

    /// The lines of the list: its sessions, how many more, and a note.
    fn line_count(&self) -> usize {
        self.board.rows.len() + usize::from(self.board.more > 0) + usize::from(self.said.is_some())
    }

    fn draw(&mut self) {
        if self
            .said
            .as_ref()
            .is_some_and(|(_, at)| self.step.wrapping_sub(*at) > NOTE_STEPS)
        {
            self.said = None;
        }
        let notch = geometry::find(&screens(self.mtm));
        self.notch = notch;
        let Some(notch) = notch.filter(|_| self.board.shown) else {
            self.hide();
            return;
        };
        let lines = self.line_count();
        let at = NSEvent::mouseLocation();
        self.open = geometry::hovering(self.open, notch, lines, (at.x, at.y));
        let frame = if self.open {
            notch.expanded(lines)
        } else {
            notch.compact()
        };
        self.panel.setFrame_display(ns_rect(frame), true);
        if let Some(layer) = self.view.layer() {
            layer.setCornerRadius(if self.open { 14.0 } else { 10.0 });
        }
        self.draw_strip(notch, frame);
        self.draw_list(notch, frame);
        if !self.visible {
            self.panel.orderFrontRegardless();
            self.visible = true;
        }
    }

    fn hide(&mut self) {
        if self.visible {
            self.panel.orderOut(None);
            self.visible = false;
        }
        self.open = false;
    }

    /// Kep and the counts in the strip under the notch, in the view's
    /// coordinates (y up): Kep on the left, the counts on the right.
    fn draw_strip(&mut self, notch: Notch, frame: Rect) {
        let text_height = 16.0;
        let y = frame.height - notch.frame.height - BAND + (BAND - text_height) / 2.0;
        let blink = self.step % 40 >= 38;
        self.face.show(
            self.board.kep.face(blink),
            if self.board.kep == Mood::Alert {
                Colour::Asking
            } else {
                Colour::White
            },
            Rect::new(10.0, y, frame.width / 2.0 - 10.0, text_height),
        );
        // From the right edge leftwards: what works, then what waits.
        let mut right = frame.width - 10.0;
        if self.board.working > 0 {
            let text = format!("{}{}", spinner(self.step), self.board.working);
            right -= self
                .working
                .fit(&text, Colour::Working, right, y, text_height)
                + 6.0;
        } else {
            self.working.hide();
        }
        if self.board.waiting > 0 {
            let text = format!("▲{}", self.board.waiting);
            self.waiting
                .fit(&text, Colour::Asking, right, y, text_height);
            self.waiting.fade(pulse(self.step));
        } else {
            self.waiting.hide();
        }
    }

    /// The list under the strip, while it is open: each session on two
    /// lines, its mark, title and time, then its project and what it is
    /// doing.
    fn draw_list(&mut self, notch: Notch, frame: Rect) {
        let rows = if self.open {
            self.board.rows.as_slice()
        } else {
            &[]
        };
        let (width, height) = (frame.width, 16.0);
        // The top of line `line`'s first and second rows of text (y up).
        let first = |line: usize| frame.height - notch.line_top(line) - 2.0 - height;
        let second = |line: usize| first(line) - 15.0;
        for (at, line) in self.lines.iter_mut().enumerate() {
            let Some(row) = rows.get(at) else {
                for label in [
                    &line.mark,
                    &line.title,
                    &line.since,
                    &line.project,
                    &line.doing,
                ] {
                    label.hide();
                }
                continue;
            };
            let (mark, colour) = match row.mark {
                Mark::Asking => ("▲", Colour::Asking),
                Mark::News => ("✓", Colour::News),
                Mark::Working => (spinner(self.step), Colour::Working),
            };
            line.mark
                .show(mark, colour, Rect::new(10.0, first(at), 14.0, height));
            line.mark.fade(if row.mark == Mark::Asking {
                pulse(self.step)
            } else {
                1.0
            });
            let since = line
                .since
                .fit(&row.since, Colour::Dim, width - 10.0, first(at), height);
            line.title.show(
                &row.title,
                Colour::White,
                Rect::new(26.0, first(at), width - 26.0 - since - 16.0, height),
            );
            // The project as wide as its name, up to half the line.
            line.project.show(
                &row.project,
                Colour::Project(row.colour),
                Rect::new(26.0, second(at), width, height),
            );
            let project = line.project.width().min((width - 36.0) / 2.0);
            line.project.show(
                &row.project,
                Colour::Project(row.colour),
                Rect::new(26.0, second(at), project, height),
            );
            line.doing.show(
                &row.doing,
                Colour::Dim,
                Rect::new(30.0 + project, second(at), width - 40.0 - project, height),
            );
        }
        let mut next = rows.len();
        if self.open && self.board.more > 0 {
            self.more.show(
                &format!("and {} more in Muster", self.board.more),
                Colour::Dim,
                Rect::new(26.0, first(next), width - 36.0, height),
            );
            next += 1;
        } else {
            self.more.hide();
        }
        match (&self.said, self.open) {
            (Some((text, _)), true) => self.note.show(
                text,
                Colour::Working,
                Rect::new(10.0, second(next), width - 20.0, 2.0 * height),
            ),
            _ => self.note.hide(),
        }
    }

    /// A click at `point`, in screen coordinates: a listed session is shown
    /// in the Muster window looked at last.
    fn clicked(&mut self, point: (f64, f64)) {
        let Some(notch) = self.notch.filter(|_| self.open) else {
            return;
        };
        let panel = notch.expanded(self.line_count());
        let Some(row) = notch
            .line_at(panel, point)
            .and_then(|line| self.board.rows.get(line))
        else {
            return;
        };
        if self.picks.try_send(row.session).is_err() {
            self.said = Some(("Muster is busy; try again.".to_owned(), self.step));
            self.draw();
        }
    }
}

/// A running session's mark, one frame a step, as in the UI.
fn spinner(step: u64) -> &'static str {
    const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    SPINNER[usize::try_from(step % 10).unwrap_or(0)]
}

/// How bright what waits on the person is at `step`: bright, plain, dim,
/// plain, four steps each, as the UI pulses it.
fn pulse(step: u64) -> f64 {
    match (step / 4) % 4 {
        0 => 1.0,
        2 => 0.45,
        _ => 0.8,
    }
}

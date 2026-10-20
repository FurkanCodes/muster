//! The notch on screen, through AppKit (`ARC-22`): a borderless panel that
//! never takes focus, above the menu bar on every Space, flush with the top
//! of the screen, black like the notch. At rest it is the notch's own height
//! with a slim side either side, the counts in them, so they read as part
//! of the notch; pointed at a moment, it eases open downwards into the
//! list (`list`), in the shape and motion of `shape`, in the colours of
//! `look`.
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

mod list;
mod look;
mod notify;
mod shape;

use std::cell::RefCell;
use std::path::PathBuf;

use dispatch2::DispatchQueue;
use objc2::rc::Retained;
use objc2::{MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSColor, NSEvent, NSFont,
    NSPanel, NSResponder, NSScreen, NSStatusWindowLevel, NSView, NSWindowCollectionBehavior,
    NSWindowStyleMask,
};
use objc2_foundation::{MainThreadMarker, NSObject, NSPoint, NSRect, NSSize};
use tokio::sync::mpsc;

use crate::board::{Board, Row};
use crate::geometry::{self, Notch, Rect, SIDE};
use crate::hover::Hover;
use crate::link::{self, Ended, FromScreen, ToScreen};
use crate::{MOVED_FOR, NotchError};
use look::{Colour, Label};

/// How long a note stays under the list, in steps: three seconds.
const NOTE_STEPS: u64 = 30;
/// The most lines the list has: its rows, how many more, and a note. The
/// panel is as big as the list with these, whatever it shows, so it never
/// changes size as the shape moves within it.
const MOST_LINES: usize = crate::board::MOST_ROWS + 2;
/// How many times a new question's count fades down and back.
const PULSES: f32 = 4.0;
/// How long the notch shows what newly needs the person, unasked, in
/// steps: three seconds.
const PEEK_STEPS: u64 = 30;

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
    let in_app = objc2_foundation::NSBundle::mainBundle()
        .bundleIdentifier()
        .is_some();
    if geometry::find(&screens(mtm)).is_none() && !in_app {
        // No screen here has a notch, and no notification can be shown.
        return Ok(());
    }
    let app = NSApplication::sharedApplication(mtm);
    // No Dock icon and no menu bar of its own: the notch and notifications
    // are all it shows.
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    let (picks, picked) = mpsc::channel(8);
    let notifier = notify::Notifier::start(&picks);
    SCREEN.with(|screen| *screen.borrow_mut() = Some(Screen::new(mtm, picks, notifier)));
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
    picked: mpsc::Receiver<FromScreen>,
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

/// What the panel is drawn as now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Drawn {
    /// Not on screen.
    Hidden,
    /// At rest: the notch and its sides.
    Rest,
    /// Open, with this many lines.
    Open(usize),
}

/// The notch on screen.
struct Screen {
    mtm: MainThreadMarker,
    panel: Retained<NSPanel>,
    shape: shape::Shape,
    asking: Label,
    working: Label,
    /// The mark beside what works, breathing.
    breath: Retained<NSView>,
    list: list::List,
    board: Board,
    step: u64,
    /// A note under the list, and the step it came at.
    said: Option<(String, u64)>,
    hover: Hover,
    drawn: Drawn,
    /// Where the panel is, once placed.
    placed: Option<Rect>,
    /// Something new needs the person, for the count to draw the eye to.
    alerted: bool,
    /// What newly needs the person, shown a moment unasked, and the step
    /// it is shown until.
    peek: Vec<Row>,
    peek_until: u64,
    /// The board the open list shows, and its lines, for a click to find.
    listed: Option<(Board, usize)>,
    /// Where the notch was when last drawn.
    notch: Option<Notch>,
    /// Where the sessions the person picks go.
    picks: mpsc::Sender<FromScreen>,
    /// Notifications of the Mac's own, where the program is Muster's app.
    notifier: Option<notify::Notifier>,
}

impl Screen {
    fn new(
        mtm: MainThreadMarker,
        picks: mpsc::Sender<FromScreen>,
        notifier: Option<notify::Notifier>,
    ) -> Self {
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
        // Clicks pass through to what is under it, but where it is drawn.
        panel.setIgnoresMouseEvents(true);

        let view = NotchView::new(mtm);
        view.setWantsLayer(true);
        if let Some(layer) = view.layer() {
            layer.setBackgroundColor(Some(&NSColor::blackColor().CGColor()));
        }
        panel.setContentView(Some(&view));
        let shape = shape::Shape::new(&view);

        let sides = NSFont::monospacedDigitSystemFontOfSize_weight(12.0, 0.3);
        let asking = Label::new(mtm, &view, &sides, false);
        let working = Label::new(mtm, &view, &sides, false);
        let breath = look::shape(mtm, &view, Colour::Working, 3.0);
        let list = list::List::new(mtm, &view);
        Self {
            mtm,
            panel,
            shape,
            asking,
            working,
            breath,
            list,
            board: Board::hidden(),
            step: 0,
            said: None,
            hover: Hover::default(),
            drawn: Drawn::Hidden,
            placed: None,
            alerted: false,
            peek: Vec::new(),
            peek_until: 0,
            listed: None,
            notch: None,
            picks,
            notifier,
        }
    }

    fn update(&mut self, update: ToScreen) {
        match update {
            ToScreen::Board { board, step } => {
                let fresh = board.fresh(&self.board);
                self.alerted |= board.asks_anew(&self.board) || !fresh.is_empty();
                if !fresh.is_empty() {
                    self.peek_until = step + PEEK_STEPS;
                }
                // What is peeked at as it is now, and only while it is.
                let mut peek: Vec<Row> = std::mem::take(&mut self.peek)
                    .iter()
                    .chain(&fresh)
                    .filter_map(|row| board.rows.iter().find(|now| now.key == row.key))
                    .cloned()
                    .collect();
                peek.dedup_by_key(|row| row.key);
                self.peek = peek;
                self.board = board;
                self.step = step;
            }
            ToScreen::Note(text) => self.said = Some((text, self.step)),
            ToScreen::Notify { session, text } => {
                if let Some(notifier) = &self.notifier {
                    notifier.post(session, &text);
                }
                return;
            }
        }
        self.draw();
    }

    /// The lines of `board`'s list: its rows, how many more, and a note.
    fn line_count(&self, board: &Board) -> usize {
        board.rows.len() + usize::from(board.more > 0) + usize::from(self.said.is_some())
    }

    /// What newly needs the person, as a board of its own, while it is
    /// shown unasked.
    fn peeking(&self) -> Option<Board> {
        (!self.hover.is_open() && !self.peek.is_empty() && self.step < self.peek_until).then(|| {
            Board {
                rows: self.peek.clone(),
                more: 0,
                ..self.board.clone()
            }
        })
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
        let at = NSEvent::mouseLocation();
        let point = (at.x, at.y);
        // Pointed at, the whole list; else, a moment, what newly needs the
        // person; else at rest.
        let peeking = self.peeking();
        let peek_lines = peeking.as_ref().map(|peek| self.line_count(peek));
        let lines = self.line_count(&self.board);
        let over = match peek_lines {
            Some(peek) if !self.hover.is_open() => notch.over(true, peek, point),
            _ => notch.over(self.hover.is_open(), lines, point),
        };
        let open = self.hover.at(over, self.step);
        if open || peeking.is_none() {
            self.peek.clear();
        }
        let listed = if open {
            Some((self.board.clone(), lines))
        } else {
            peeking.zip(peek_lines)
        };
        let frame = self.place(notch);
        self.reshape(notch, frame, listed.as_ref().map(|(_, lines)| *lines));
        // It takes clicks only where it is drawn.
        let drawn_lines = listed.as_ref().map(|(_, lines)| *lines);
        self.panel.setIgnoresMouseEvents(!notch.over(
            drawn_lines.is_some(),
            drawn_lines.unwrap_or(0),
            point,
        ));
        self.draw_sides(notch, frame);
        // Closing, the list is drawn as it was until it has faded.
        if let Some((board, lines)) = &listed {
            self.list.draw(&list::Drawn {
                board,
                notch,
                frame,
                lines: *lines,
                pointed: notch.line_at(*lines, point),
                note: self.said.as_ref().map(|(text, _)| text.as_str()),
            });
        }
        self.listed = listed;
        if !self.panel.isVisible() {
            self.panel.orderFrontRegardless();
        }
    }

    /// Puts the panel where it goes, as big as the list at its longest, its
    /// top at the screen's edge; returns where it is.
    fn place(&mut self, notch: Notch) -> Rect {
        let frame = notch.open(MOST_LINES);
        if self.placed != Some(frame) {
            self.panel.setFrame_display(ns_rect(frame), true);
            self.shape.place(frame);
            self.placed = Some(frame);
            // A new place: the shape is drawn afresh there.
            if self.drawn != Drawn::Hidden {
                self.drawn = Drawn::Rest;
                self.shape.set(&notch.outline(None, frame), None);
            }
        }
        frame
    }

    /// Gives the panel its shape, open with `lines` lines or at rest,
    /// moving from the shape it has.
    fn reshape(&mut self, notch: Notch, frame: Rect, open: Option<usize>) {
        let wanted = open.map_or(Drawn::Rest, Drawn::Open);
        if wanted == self.drawn {
            return;
        }
        let motion = match (self.drawn, wanted) {
            (Drawn::Hidden, _) => None,
            (Drawn::Open(_), Drawn::Open(_)) => Some(shape::Motion::Resize),
            (_, Drawn::Open(_)) => Some(shape::Motion::Open),
            _ => Some(shape::Motion::Close),
        };
        self.shape.set(&notch.outline(open, frame), motion);
        match (self.drawn, wanted) {
            (Drawn::Open(_), Drawn::Open(_)) => {}
            (_, Drawn::Open(_)) => self.list.reveal(),
            (Drawn::Open(_), _) => self.list.conceal(),
            _ => {}
        }
        self.drawn = wanted;
    }

    fn hide(&mut self) {
        if self.drawn != Drawn::Hidden {
            self.panel.orderOut(None);
            self.drawn = Drawn::Hidden;
            self.list.conceal();
        }
        self.hover.close();
        if let Some(layer) = self.breath.layer() {
            shape::still(&layer);
        }
    }

    /// The counts in the slim sides, level with the notch, in the view's
    /// coordinates (y up), each centred in its side: the questions asked
    /// of the person on the left, drawing the eye a moment when a new one
    /// comes; on the right what works, its mark breathing, or once nothing
    /// works, what finished unseen.
    fn draw_sides(&mut self, notch: Notch, frame: Rect) {
        let height = 16.0;
        let compact = notch.compact();
        let (left, right) = (compact.x - frame.x, compact.max_x() - frame.x);
        let y = frame.height - notch.frame.height + (notch.frame.height - height) / 2.0;
        let board = &self.board;
        if board.asking > 0 {
            self.asking.centre(
                &format!("▲{}", board.asking),
                Colour::Asking,
                left + SIDE / 2.0 + 1.0,
                y,
                height,
            );
            self.asking.say(&format!("{} asking you", board.asking));
        } else if board.troubles > 0 {
            // No question, but trouble in the work.
            self.asking.centre(
                &format!("!{}", board.troubles),
                Colour::Asking,
                left + SIDE / 2.0 + 1.0,
                y,
                height,
            );
            self.asking
                .say(&format!("{} conflicts or failed checks", board.troubles));
        } else {
            self.asking.hide();
        }
        if self.alerted
            && (board.asking > 0 || board.troubles > 0)
            && let Some(layer) = self.asking.field.layer()
        {
            shape::pulse(&layer, PULSES);
        }
        self.alerted = false;

        let middle = right - SIDE / 2.0 - 1.0;
        if board.working > 0 {
            // The count and its mark, centred together.
            let count = board.working.to_string();
            self.working
                .show(&count, Colour::Working, Rect::new(0.0, y, 1.0, height));
            let width = self.working.width() + 3.0 + 6.0;
            let start = middle - width / 2.0;
            self.working.show(
                &count,
                Colour::Working,
                Rect::new(start, y, width - 9.0, height),
            );
            self.working.say(&format!("{} working", board.working));
            self.breath
                .setFrame(ns_rect(Rect::new(start + width - 6.0, y + 5.0, 6.0, 6.0)));
            self.breath.setHidden(false);
            if let Some(layer) = self.breath.layer()
                && !shape::moving(&layer)
            {
                shape::breathe(&layer);
            }
        } else {
            self.breath.setHidden(true);
            if let Some(layer) = self.breath.layer() {
                shape::still(&layer);
            }
            if board.news > 0 {
                self.working
                    .centre(&format!("✓{}", board.news), Colour::News, middle, y, height);
                self.working.say(&format!("{} finished", board.news));
            } else {
                self.working.hide();
            }
        }
    }

    /// A click at `point`, in screen coordinates: on a listed line it
    /// shows its session in the Muster window looked at last, and the list
    /// closes; elsewhere on the closed notch it opens the list at once.
    fn clicked(&mut self, point: (f64, f64)) {
        let Some(notch) = self.notch else {
            return;
        };
        let row = self.listed.as_ref().and_then(|(board, lines)| {
            notch
                .line_at(*lines, point)
                .and_then(|line| board.rows.get(line))
        });
        let Some(row) = row else {
            if !self.hover.is_open() && notch.compact().contains(point) {
                self.hover.open_now();
                self.peek.clear();
                self.draw();
            }
            return;
        };
        if self
            .picks
            .try_send(FromScreen::Picked(row.session))
            .is_err()
        {
            self.said = Some(("Muster is busy; try again.".to_owned(), self.step));
        } else {
            self.hover.close();
            self.peek.clear();
        }
        self.draw();
    }
}

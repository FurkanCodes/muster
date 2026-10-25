//! Running the app: the host terminal, the link to the daemon, and the loop
//! that feeds the app events and draws it.

use std::io::{Stdout, stdout};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crossterm::event::{
    DisableBracketedPaste, DisableFocusChange, EnableBracketedPaste, EnableFocusChange, Event,
    EventStream, KeyboardEnhancementFlags, MouseEventKind, PopKeyboardEnhancementFlags,
    PushKeyboardEnhancementFlags,
};
use crossterm::execute;
use crossterm::terminal::{Clear, ClearType, EnterAlternateScreen};
use futures_util::StreamExt;
use muster_core::protocol::TermSize;
use ratatui::DefaultTerminal;
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::mpsc;
use tokio_util::task::AbortOnDropHandle;

use crate::app::{Action, App};
use crate::clipboard;
use crate::colours;
use crate::folders::{self, Folder};
use crate::motion;
use crate::pointer_shape::{Shape, Shapes};
use crate::view;

mod drawing;
mod link;

use drawing::Draws;

use self::link::{LINK_EVENTS, LinkEvent, Outgoing, keep_linked};
pub use self::link::{START_AGAIN_AFTER, START_WAIT};

/// Connects to a daemon some other way than its socket here: another
/// machine's, through ssh.
pub type Connector = Box<
    dyn Fn() -> std::pin::Pin<
            Box<
                dyn std::future::Future<
                        Output = Result<muster_client::Connection, muster_client::ClientError>,
                    > + Send,
            >,
        > + Send
        + Sync,
>;

/// How the app finds, and if need be starts, the daemon.
pub struct Options {
    pub socket: PathBuf,
    /// Connects instead of the socket, as many times as the link is lost:
    /// to another machine's daemon (`machine`).
    pub connect: Option<Connector>,
    /// The machine whose Muster this is, where `connect` reaches another;
    /// shown, and nothing of this machine's (its folders, its notch) is
    /// offered for it.
    pub machine: Option<String>,
    /// Starts a daemon when none answers. `None` never starts one.
    pub start_daemon: Option<Box<dyn Fn() -> std::io::Result<()> + Send + Sync>>,
    /// Where `muster` was started, offered when adding a project.
    pub cwd: PathBuf,
    /// Notify the person of news while their terminal window is not in
    /// focus.
    pub notify: bool,
    /// Let running sessions' marks spin and what needs the person pulse.
    pub motion: bool,
    /// Starts Muster's notch, unless it runs already; `None` where there
    /// is none (`ARC-22`).
    pub start_notch: Option<Box<dyn Fn() -> std::io::Result<()> + Send + Sync>>,
    /// The application the person's terminal is, by its bundle identifier
    /// (`com.googlecode.iterm2`), brought to the front when the person
    /// picks a session in the notch.
    pub terminal_app: Option<String>,
    /// The session to open, with the keys in it, once the state shows it
    /// (`muster attach`).
    pub open: Option<muster_core::SessionId>,
}

impl std::fmt::Debug for Options {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Options")
            .field("socket", &self.socket)
            .field("machine", &self.machine)
            .field("connects", &self.connect.is_some())
            .field("starts_daemon", &self.start_daemon.is_some())
            .field("cwd", &self.cwd)
            .field("notify", &self.notify)
            .field("motion", &self.motion)
            .field("starts_notch", &self.start_notch.is_some())
            .field("terminal_app", &self.terminal_app)
            .field("open", &self.open)
            .finish()
    }
}

/// Why the UI could not run.
#[derive(Debug, thiserror::Error)]
pub enum TuiError {
    #[error("terminal error: {0}")]
    Terminal(#[from] std::io::Error),
}

/// What woke the loop.
enum Woke {
    Input(Option<std::io::Result<Event>>),
    Link(LinkEvent),
    Folders((PathBuf, Result<Vec<Folder>, String>)),
    /// The person's window may have another size.
    Window,
    /// Something of its own was due: a frame, a step or the clock.
    Time,
}

/// The redraw rate's upper bound.
const FRAME: Duration = Duration::from_millis(16);
/// The longest drawing waits for a program to finish a frame it started
/// (synchronized output), in case one never does.
const FRAME_WAIT: Duration = Duration::from_millis(150);

/// Runs the UI until the person quits. The daemon, and every session, keep
/// running.
pub async fn run(mut options: Options) -> Result<(), TuiError> {
    // Heard from before the window's size is first read, so no change to
    // it goes unseen, not even one before crossterm listens.
    let mut window_changes = signal(SignalKind::window_change())?;
    let mut terminal = HostTerminal::enter()?;
    let size = area_size(terminal.area.width, terminal.area.height);
    let mut app = App::new(options.cwd.clone(), size);
    app.person.colours = std::mem::take(&mut terminal.colours);
    app.person.cell_pixels = cell_pixels();
    app.alarms.notify = options.notify;
    app.moving.on = options.motion;
    app.person.graphics = crate::view::logos::Graphics::detect(|name| std::env::var(name).ok());
    app.welcome = true;
    if let Some(session) = options.open {
        app.open_at_start(session);
    }
    let outside = Outside::take(&mut options);
    app.person.notch_here = outside.start_notch.is_some();
    app.machine.clone_from(&outside.machine);

    // Bounded: a loop that falls behind stops the link reading, and the
    // daemon treats this client as any slow one (`ARC-14`). The loop never
    // waits on the daemon in turn: its frames go out through the link.
    let (link_events, mut link) = mpsc::channel(LINK_EVENTS);
    // Ends when dropped, as the UI ends (`ARC-13`).
    #[expect(
        clippy::disallowed_methods,
        reason = "ARC-13: the AbortOnDropHandle owns it"
    )]
    let _link_task = AbortOnDropHandle::new(tokio::spawn(keep_linked(options, link_events)));

    // Folders listed for the folder picker, off this loop: a folder on a
    // slow disk must not stop the UI.
    #[expect(
        clippy::disallowed_methods,
        reason = "ARC-14: one listing for each folder the person opens in the picker"
    )]
    let (listed_folders, mut folder_lists) =
        mpsc::unbounded_channel::<(PathBuf, Result<Vec<Folder>, String>)>();

    let mut input = EventStream::new();
    let mut sender: Option<Outgoing> = None;
    let mut draws = Draws::new(monotonic());
    let mut steps = motion::Steps::new(monotonic());
    let mut pointer = Pointer::detect();
    let outcome = loop {
        // Asleep until an event or something of its own is due: idle, the
        // UI wakes for a time on screen to change, and not at all with no
        // sessions.
        let now = monotonic();
        let wake = next_wake(
            now,
            wall_clock().as_millis(),
            app.clock_every(),
            app.wants_steps().then(|| steps.next(now)),
            draws.wake(),
        );
        let woke = tokio::select! {
            event = input.next() => Woke::Input(event),
            Some(event) = link.recv() => Woke::Link(event),
            Some(listed) = folder_lists.recv() => Woke::Folders(listed),
            Some(()) = window_changes.recv() => Woke::Window,
            () = sleep_until(wake) => Woke::Time,
        };
        // The step and the clock first, so what an event starts is timed
        // from now.
        let due = tick(&mut app, &mut steps, &mut draws.dirty);
        draws.woke(&woke, monotonic());
        let actions = match woke {
            Woke::Input(event) => match event {
                Some(Ok(Event::Key(key))) => app.on_key(key),
                Some(Ok(Event::Paste(text))) => app.on_paste(&text),
                // A move that lights nothing new and sends nothing draws
                // nothing: the pointer crossing the window costs no frame.
                Some(Ok(Event::Mouse(mouse))) if mouse.kind == MouseEventKind::Moved => {
                    let was = (app.panels.hover.clone(), app.lit_control());
                    let actions = app.on_mouse(mouse);
                    let now = (app.panels.hover.clone(), app.lit_control());
                    if actions.is_empty() && now == was && due.is_empty() {
                        draw_if_due(&mut terminal, &app, &mut draws)?;
                        continue;
                    }
                    actions
                }
                Some(Ok(Event::Mouse(mouse))) => app.on_mouse(mouse),
                Some(Ok(Event::FocusGained)) => app.on_focus(true),
                Some(Ok(Event::FocusLost)) => app.on_focus(false),
                Some(Ok(Event::Resize(..))) => follow_window(&mut terminal, &mut app)?,
                Some(Err(error)) => break Err(TuiError::Terminal(error)),
                None => break Ok(()),
            },
            Woke::Link(event) => linked(&mut app, &mut sender, event),
            Woke::Folders((dir, folders)) => app.on_folders(&dir, folders),
            Woke::Window => follow_window(&mut terminal, &mut app)?,
            Woke::Time if due.is_empty() => {
                draw_if_due(&mut terminal, &app, &mut draws)?;
                continue;
            }
            Woke::Time => Vec::new(),
        };
        let actions: Vec<_> = due.into_iter().chain(actions).collect();
        draws.dirty = true;
        draws.actions(&actions);
        let quit = perform(actions, &mut sender, &listed_folders, &outside);
        if quit {
            break Ok(());
        }
        pointer.follow(&app);
        draw_if_due(&mut terminal, &app, &mut draws)?;
    };
    drop(pointer);
    drop(terminal);
    outcome
}

/// What the daemon's link brings: the app connected, a frame, or the
/// connection lost.
fn linked(app: &mut App, sender: &mut Option<Outgoing>, event: LinkEvent) -> Vec<Action> {
    match event {
        LinkEvent::Connected {
            sender: connected,
            welcome,
        } => {
            *sender = Some(connected);
            app.on_connected(welcome.daemon_version)
        }
        LinkEvent::Frame(frame) => app.on_frame(*frame),
        LinkEvent::Lost(reason) => {
            *sender = None;
            app.on_disconnected(reason);
            Vec::new()
        }
    }
}

/// The pointer's shape as last asked for, in the names the terminal knows,
/// if it takes them; the terminal's own given back as it is dropped, as
/// the UI ends.
struct Pointer {
    shapes: Option<Shapes>,
    shown: Shape,
}

impl Pointer {
    fn detect() -> Self {
        Self {
            shapes: Shapes::detect(|name| std::env::var(name).ok()),
            shown: Shape::Default,
        }
    }

    /// Asks the terminal for the shape `app` wants, if not the one asked
    /// for last.
    fn follow(&mut self, app: &App) {
        self.show(app.pointer_shape());
    }

    fn show(&mut self, shape: Shape) {
        use std::io::Write;
        let Some(shapes) = self.shapes else {
            return;
        };
        if shape != self.shown {
            self.shown = shape;
            let mut out = stdout();
            let _ = out.write_all(shapes.sequence(shape).as_bytes());
            let _ = out.flush();
        }
    }
}

impl Drop for Pointer {
    fn drop(&mut self) {
        self.show(Shape::Default);
    }
}

/// Hands the app the step and the clock as they are now, marking what
/// they change to draw; and what a held finish, due now, tells.
fn tick(app: &mut App, steps: &mut motion::Steps, dirty: &mut bool) -> Vec<Action> {
    if let Some(step) = steps.due(monotonic())
        && app.on_step(step)
    {
        *dirty = true;
    }
    if app.on_clock(wall_clock()) {
        *dirty = true;
    }
    app.due_news()
}

/// What the runtime reaches outside the UI and its daemon.
struct Outside {
    start_notch: Option<Box<dyn Fn() -> std::io::Result<()> + Send + Sync>>,
    terminal_app: Option<String>,
    /// Another machine's Muster is shown: this one's folders are not its.
    machine: Option<String>,
}

impl Outside {
    /// What `options` say of it; the rest go to the link to the daemon.
    fn take(options: &mut Options) -> Self {
        Self {
            start_notch: options.start_notch.take(),
            terminal_app: options.terminal_app.take(),
            machine: options.machine.clone(),
        }
    }
}

/// Carries out what the app asked for, never waiting on the daemon;
/// whether it asked to quit.
fn perform(
    actions: Vec<Action>,
    sender: &mut Option<Outgoing>,
    listed_folders: &mpsc::UnboundedSender<(PathBuf, Result<Vec<Folder>, String>)>,
    outside: &Outside,
) -> bool {
    let mut quit = false;
    for action in actions {
        match action {
            Action::Quit => quit = true,
            Action::Notify(text) => notify(&text),
            Action::Open(url) => open_in_browser(&url),
            Action::Hear(sound) => hear(&sound),
            Action::Copy(text) => clipboard::copy(&text),
            Action::StartNotch => {
                if let Some(start) = &outside.start_notch {
                    // Nothing to tell the person: the notch is a nicety.
                    let _ = start();
                }
            }
            Action::Activate => {
                if let Some(app) = &outside.terminal_app {
                    bring_to_front(app);
                }
            }
            Action::ListFolders(dir) if outside.machine.is_some() => {
                let machine = outside.machine.as_deref().unwrap_or_default();
                let _ = listed_folders.send((
                    dir,
                    Err(format!(
                        "the folders are {machine}'s, which this window cannot list: type the \
                         path there"
                    )),
                ));
            }
            Action::ListFolders(dir) => {
                let listed = listed_folders.clone();
                tokio::task::spawn_blocking(move || {
                    let folders = folders::list(&dir);
                    let _ = listed.send((dir, folders));
                });
            }
            Action::Send(frame) => {
                if let Some(connected) = sender
                    && !connected.send(frame)
                {
                    // The link task notices and reconnects.
                    *sender = None;
                }
            }
        }
    }
    quit
}

/// Draws what changed, at most once a `FRAME`, and not while the session
/// shown is midway through a frame of its own.
fn draw_if_due(terminal: &mut HostTerminal, app: &App, draws: &mut Draws) -> Result<(), TuiError> {
    let now = monotonic();
    if draws.due(app.frame_pending(), now) {
        terminal.terminal.draw(|frame| view::draw(frame, app))?;
        draws.drawn(now);
    }
    Ok(())
}

/// When the loop next has something of its own to do: draw a frame that is
/// due, take the next step of something moving, or show a time on screen
/// that reads differently. `None`: nothing, until an event comes.
fn next_wake(
    now: Instant,
    wall_millis: i64,
    clock_every: Option<i64>,
    next_step: Option<Instant>,
    next_draw: Option<Instant>,
) -> Option<Instant> {
    let clock = clock_every.filter(|every| *every > 0).map(|every| {
        let left = every - wall_millis.rem_euclid(every);
        now + Duration::from_millis(u64::try_from(left).unwrap_or(0))
    });
    [clock, next_step, next_draw].into_iter().flatten().min()
}

/// Sleeps until `wake`, or for good.
async fn sleep_until(wake: Option<Instant>) {
    match wake {
        Some(at) => tokio::time::sleep_until(at.into()).await,
        None => std::future::pending().await,
    }
}

/// The time now, for the loop's own timing: read here, so the app and the
/// view never read the clock (`ARC-03`).
#[expect(
    clippy::disallowed_methods,
    reason = "ARC-03: the UI's one monotonic clock"
)]
fn monotonic() -> Instant {
    Instant::now()
}

/// The time now, for how long sessions have run or waited: read here, so
/// the app and the view never read the clock (`ARC-03`).
#[expect(clippy::disallowed_methods, reason = "ARC-03: the UI's one wall clock")]
fn wall_clock() -> muster_core::Timestamp {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| {
            i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
        });
    muster_core::Timestamp::from_millis(millis)
}

/// Holds drawing while the session shown draws a frame, as a terminal with
/// synchronized output does, so a frame is seen whole; never longer than
/// `FRAME_WAIT`.
#[derive(Debug, Default)]
struct FrameHold {
    since: Option<Instant>,
}

impl FrameHold {
    fn may_draw(&mut self, frame_pending: bool, now: Instant) -> bool {
        if !frame_pending {
            self.since = None;
            return true;
        }
        let since = *self.since.get_or_insert(now);
        if now.duration_since(since) < FRAME_WAIT {
            return false;
        }
        self.since = None;
        true
    }
}

/// Follows the person's window to the size it has now, its cells perhaps
/// of another size in pixels too. Read again at each sign of a change, the
/// UI's own window-change signal or crossterm's resize event, rather than
/// taken from the event, whose size can be older than the signal's read.
fn follow_window(terminal: &mut HostTerminal, app: &mut App) -> std::io::Result<Vec<Action>> {
    let (cols, rows) = crossterm::terminal::size()?;
    let cell = cell_pixels();
    if (cols, rows) == (terminal.area.width, terminal.area.height) && cell == app.person.cell_pixels
    {
        return Ok(Vec::new());
    }
    terminal.resize(cols, rows)?;
    app.person.cell_pixels = cell;
    Ok(app.on_resize(area_size(cols, rows)))
}

/// One cell of the person's terminal in pixels, from the size the system
/// keeps for it; `None` where the terminal says no pixels, as some do not.
fn cell_pixels() -> Option<muster_core::protocol::PixelSize> {
    let size = crossterm::terminal::window_size().ok()?;
    cell_of(size.width, size.height, size.columns, size.rows)
}

/// A cell's pixels in a window `width` by `height` pixels and `columns` by
/// `rows` cells, if both are known.
fn cell_of(
    width: u16,
    height: u16,
    columns: u16,
    rows: u16,
) -> Option<muster_core::protocol::PixelSize> {
    let cell = muster_core::protocol::PixelSize {
        width: width.checked_div(columns)?,
        height: height.checked_div(rows)?,
    };
    (cell.width > 0 && cell.height > 0).then_some(cell)
}

fn area_size(cols: u16, rows: u16) -> TermSize {
    let area = view::terminal_area(ratatui::layout::Rect::new(0, 0, cols, rows));
    TermSize {
        cols: area.width,
        rows: area.height,
    }
}

/// The person's terminal while the UI runs: raw mode, the alternate screen,
/// bracketed paste, mouse and focus reports, and the kitty keyboard protocol
/// where the terminal has it (so Shift+Enter reaches Claude Code). Put back
/// on drop and on panic.
struct HostTerminal {
    terminal: DefaultTerminal,
    area: ratatui::layout::Rect,
    enhanced_keys: bool,
    /// Its colours, as it said them.
    colours: muster_core::colours::TerminalColours,
}

impl HostTerminal {
    fn resize(&mut self, cols: u16, rows: u16) -> std::io::Result<()> {
        self.area = ratatui::layout::Rect::new(0, 0, cols, rows);
        self.terminal.resize(self.area)
    }

    fn enter() -> std::io::Result<Self> {
        let (cols, rows) = crossterm::terminal::size()?;
        let area = ratatui::layout::Rect::new(0, 0, cols, rows);
        let terminal = ratatui::init_with_options(ratatui::TerminalOptions {
            viewport: ratatui::Viewport::Fixed(area),
        });
        let mut out = stdout();
        // A screen of its own, cleared, so nothing of the shell's shows
        // through the cells a frame leaves blank, and the shell's screen is
        // back as it was on quitting. ratatui enters it only with the
        // default viewport; its own clear would ask where the cursor is.
        execute!(out, EnterAlternateScreen, Clear(ClearType::All))?;
        // The mouse scrolls sessions back, picks lines and selects a
        // session's text; the person's terminal's own selection, of whole
        // lines of the window, then takes a modifier (Option or Shift).
        execute!(
            out,
            EnableBracketedPaste,
            MouseReports(true),
            EnableFocusChange
        )?;
        let keyboard = crossterm::terminal::supports_keyboard_enhancement();
        let enhanced_keys = matches!(keyboard, Ok(true));
        if enhanced_keys {
            execute!(
                out,
                PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
            )?;
        }
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            restore_modes(&mut stdout(), enhanced_keys);
            previous(info);
        }));
        // Asked before anything else reads the terminal's input, and only
        // of a terminal that answered the keyboard question in time: one
        // that answers nothing would only keep the UI waiting.
        let colours = if keyboard.is_ok() {
            colours::ask(monotonic() + colours::WAIT, monotonic)
        } else {
            muster_core::colours::TerminalColours::default()
        };
        Ok(Self {
            terminal,
            area,
            enhanced_keys,
            colours,
        })
    }
}

impl Drop for HostTerminal {
    fn drop(&mut self) {
        restore_modes(&mut stdout(), self.enhanced_keys);
        ratatui::restore();
    }
}

fn restore_modes(out: &mut Stdout, enhanced_keys: bool) {
    if enhanced_keys {
        let _ = execute!(out, PopKeyboardEnhancementFlags);
    }
    let _ = execute!(
        out,
        DisableBracketedPaste,
        MouseReports(false),
        DisableFocusChange
    );
}

/// Shows `text` as a desktop notification where the terminal supports it
/// (OSC 9: iTerm2, `WezTerm`, Ghostty, kitty, Windows Terminal) and rings its
/// bell, which the others show as a badge or a bounce. The text can carry
/// an issue's title, so control characters are taken out first: they could
/// end the sequence and send the terminal commands of their own.
fn notify(text: &str) {
    use std::io::Write;
    let mut out = stdout();
    let _ = write!(out, "\x1b]9;Muster: {}\x07\x07", printable(text));
    let _ = out.flush();
}

/// Opens a web page in the person's browser, with the system's own
/// command; only a web address, as a ticket's link should be.
fn open_in_browser(url: &str) {
    if !is_web_address(url) {
        return;
    }
    let program = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let _ = std::process::Command::new(program)
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|mut child| {
            // Reaped in the background: it returns at once.
            let _ = std::thread::Builder::new()
                .name("muster-open-wait".to_owned())
                .spawn(move || child.wait());
        });
}

/// Plays `sound`, one of the Mac's own or of the person's in
/// `~/Library/Sounds`, as they choose it for a notification, with the
/// system's own `afplay`; macOS only, and only a name with no folder in it.
fn hear(sound: &muster_core::telling::Sound) {
    use muster_core::telling::{MAC_SOUNDS, Sound};
    let Sound::Named(name) = sound else {
        return;
    };
    if !cfg!(target_os = "macos") || name.contains(['/', '\\']) || name.starts_with('.') {
        return;
    }
    let file = if MAC_SOUNDS.contains(&name.as_str()) {
        PathBuf::from(format!("/System/Library/Sounds/{name}.aiff"))
    } else {
        match std::env::var_os("HOME") {
            Some(home) => PathBuf::from(home).join("Library/Sounds").join(name),
            None => return,
        }
    };
    let _ = std::process::Command::new("/usr/bin/afplay")
        .arg(file)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|mut child| {
            // Reaped in the background: it plays for a moment.
            let _ = std::thread::Builder::new()
                .name("muster-hear-wait".to_owned())
                .spawn(move || child.wait());
        });
}

/// Brings the application with bundle identifier `app` to the front, with
/// the system's own `open` (found on `PATH`, so a test stands in for it);
/// macOS only, and only an identifier, as the environment could hold
/// anything.
fn bring_to_front(app: &str) {
    if !cfg!(target_os = "macos") || !is_bundle_id(app) {
        return;
    }
    let _ = std::process::Command::new("open")
        .args(["-b", app])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|mut child| {
            // Reaped in the background: it returns at once.
            let _ = std::thread::Builder::new()
                .name("muster-open-wait".to_owned())
                .spawn(move || child.wait());
        });
}

/// Whether `app` reads as a bundle identifier, `com.example.App`: letters,
/// digits, dots and hyphens, and no `-` first, which `open` would take for
/// an option.
fn is_bundle_id(app: &str) -> bool {
    !app.is_empty()
        && !app.starts_with('-')
        && app
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
}

/// Whether `url` is an http or https address and nothing else: a ticket's
/// link could otherwise name a file or an application to open.
fn is_web_address(url: &str) -> bool {
    (url.starts_with("https://") || url.starts_with("http://"))
        && !url.chars().any(|c| c.is_control() || c.is_whitespace())
}

/// `text` without control characters.
fn printable(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).collect()
}

/// Asks the person's terminal for mouse presses, releases and the wheel,
/// and every movement, in SGR encoding: a move lights what a drag would
/// move, and reaches a program that asked for it; one that changes neither
/// draws nothing.
struct MouseReports(bool);

impl crossterm::Command for MouseReports {
    fn write_ansi(&self, out: &mut impl std::fmt::Write) -> std::fmt::Result {
        let switch = if self.0 { 'h' } else { 'l' };
        write!(
            out,
            "\x1b[?1000{switch}\x1b[?1003{switch}\x1b[?1006{switch}"
        )
    }
}

#[cfg(test)]
#[expect(clippy::disallowed_methods, reason = "tests time themselves")]
mod tests {
    #[test]
    fn only_a_bundle_identifier_is_handed_to_open() {
        assert!(super::is_bundle_id("com.googlecode.iterm2"));
        assert!(super::is_bundle_id("com.mitchellh.ghostty"));
        for not_one in [
            "",
            "-a",
            "com.apple.Terminal; rm",
            "a b",
            "/Applications/X.app",
        ] {
            assert!(!super::is_bundle_id(not_one), "{not_one}");
        }
    }

    #[test]
    fn drawing_waits_for_a_frame_to_finish_but_not_for_ever() {
        use std::time::{Duration, Instant};
        let mut hold = super::FrameHold::default();
        let start = Instant::now();
        assert!(hold.may_draw(false, start));
        assert!(!hold.may_draw(true, start));
        assert!(!hold.may_draw(true, start + Duration::from_millis(100)));
        // Finished: drawn at once.
        assert!(hold.may_draw(false, start + Duration::from_millis(110)));
        // Never finished: drawn anyway once the wait is up.
        assert!(!hold.may_draw(true, start + Duration::from_millis(200)));
        assert!(hold.may_draw(true, start + Duration::from_millis(350)));
    }

    #[test]
    fn with_nothing_due_the_ui_sleeps_until_an_event() {
        let now = std::time::Instant::now();
        assert_eq!(super::next_wake(now, 12_345, None, None, None), None);
    }

    #[test]
    fn the_ui_wakes_for_the_first_thing_due() {
        use std::time::{Duration, Instant};
        let now = Instant::now();
        let ms = Duration::from_millis;
        // A time on screen that reads by the second: at the next one.
        assert_eq!(
            super::next_wake(now, 12_300, Some(1_000), None, None),
            Some(now + ms(700))
        );
        // By the minute.
        assert_eq!(
            super::next_wake(now, 60_000 + 1_000, Some(60_000), None, None),
            Some(now + ms(59_000))
        );
        // A step or a frame due sooner comes first.
        assert_eq!(
            super::next_wake(now, 12_300, Some(1_000), Some(now + ms(40)), None),
            Some(now + ms(40))
        );
        assert_eq!(
            super::next_wake(
                now,
                12_300,
                Some(1_000),
                Some(now + ms(40)),
                Some(now + ms(16))
            ),
            Some(now + ms(16))
        );
    }

    #[test]
    fn a_cell_s_pixels_come_from_the_window_s_where_it_says_them() {
        let cell = |width, height| Some(muster_core::protocol::PixelSize { width, height });
        assert_eq!(super::cell_of(960, 576, 120, 36), cell(8, 16));
        // A window a little larger than its cells, as with margins.
        assert_eq!(super::cell_of(965, 580, 120, 36), cell(8, 16));
        assert_eq!(super::cell_of(0, 0, 120, 36), None, "no pixels said");
        assert_eq!(super::cell_of(960, 576, 0, 0), None);
    }

    #[test]
    fn notifications_carry_no_control_characters() {
        assert_eq!(
            super::printable("#7 evil\x1b]0;owned\x07title\u{9b}31m\n"),
            "#7 evil]0;ownedtitle31m"
        );
    }

    #[test]
    fn only_web_addresses_are_opened() {
        assert!(super::is_web_address("https://github.com/o/app/issues/42"));
        assert!(super::is_web_address(
            "http://jira.example.com/browse/APP-7"
        ));
        for refused in [
            "file:///etc/passwd",
            "/Applications/Calculator.app",
            "javascript:alert(1)",
            "https://example.com/\nrm",
            "https://example.com/ two",
        ] {
            assert!(!super::is_web_address(refused), "{refused}");
        }
    }
}

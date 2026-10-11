//! Running the app: the host terminal, the link to the daemon, and the loop
//! that feeds the app events and draws it.

use std::io::{Stdout, stdout};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crossterm::event::{
    DisableBracketedPaste, DisableFocusChange, EnableBracketedPaste, EnableFocusChange, Event,
    EventStream, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
    PushKeyboardEnhancementFlags,
};
use crossterm::execute;
use futures_util::StreamExt;
use muster_client::{ClientError, Connection, FrameSender};
use muster_core::protocol::{ServerFrame, TermSize, Welcome};
use ratatui::DefaultTerminal;
use tokio::sync::mpsc;

use crate::app::{Action, App};
use crate::view;

/// How the app finds, and if need be starts, the daemon.
pub struct Options {
    pub socket: PathBuf,
    /// Starts a daemon when none answers. `None` never starts one.
    pub start_daemon: Option<Box<dyn Fn() -> std::io::Result<()> + Send + Sync>>,
    /// Where `muster` was started, offered when adding a project.
    pub cwd: PathBuf,
    /// Notify the person of news while their terminal window is not in
    /// focus.
    pub notify: bool,
}

impl std::fmt::Debug for Options {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Options")
            .field("socket", &self.socket)
            .field("starts_daemon", &self.start_daemon.is_some())
            .field("cwd", &self.cwd)
            .field("notify", &self.notify)
            .finish()
    }
}

/// Why the UI could not run.
#[derive(Debug, thiserror::Error)]
pub enum TuiError {
    #[error("terminal error: {0}")]
    Terminal(#[from] std::io::Error),
}

/// What the link task reports.
enum LinkEvent {
    Connected {
        sender: FrameSender,
        welcome: Welcome,
    },
    Frame(Box<ServerFrame>),
    Lost(String),
}

/// The redraw rate's upper bound.
const FRAME: Duration = Duration::from_millis(16);
const CLIENT: &str = concat!("muster-tui ", env!("CARGO_PKG_VERSION"));

/// Runs the UI until the person quits. The daemon, and every session, keep
/// running.
pub async fn run(options: Options) -> Result<(), TuiError> {
    let mut terminal = HostTerminal::enter()?;
    let size = terminal_size(&terminal.terminal)?;
    let mut app = App::new(options.cwd.clone(), size);
    app.notify = options.notify;

    let (link_events, mut link) = mpsc::unbounded_channel();
    let link_task = tokio::spawn(keep_linked(options, link_events));

    let mut input = EventStream::new();
    let mut sender: Option<FrameSender> = None;
    let mut redraw = tokio::time::interval(FRAME);
    let mut last_draw = Instant::now();
    let mut dirty = true;
    let outcome = loop {
        let actions = tokio::select! {
            event = input.next() => match event {
                Some(Ok(Event::Key(key))) => app.on_key(key),
                Some(Ok(Event::Paste(text))) => app.on_paste(&text),
                Some(Ok(Event::Mouse(mouse))) => app.on_mouse(mouse),
                Some(Ok(Event::FocusGained)) => app.on_focus(true),
                Some(Ok(Event::FocusLost)) => app.on_focus(false),
                Some(Ok(Event::Resize(cols, rows))) => app.on_resize(area_size(cols, rows)),
                Some(Err(error)) => break Err(TuiError::Terminal(error)),
                None => break Ok(()),
            },
            Some(event) = link.recv() => match event {
                LinkEvent::Connected { sender: connected, welcome } => {
                    sender = Some(connected);
                    app.on_connected(welcome.daemon_version)
                }
                LinkEvent::Frame(frame) => app.on_frame(*frame),
                LinkEvent::Lost(reason) => {
                    sender = None;
                    app.on_disconnected(reason);
                    Vec::new()
                }
            },
            _ = redraw.tick() => {
                // A run's time goes on while it is shown.
                let ticking = app.focused_run().is_some() && last_draw.elapsed() >= Duration::from_secs(1);
                if dirty || ticking {
                    app.now = wall_clock();
                    terminal.terminal.draw(|frame| view::draw(frame, &app))?;
                    dirty = false;
                    last_draw = Instant::now();
                }
                continue;
            }
        };
        dirty = true;
        let mut quit = false;
        for action in actions {
            match action {
                Action::Quit => quit = true,
                Action::Notify(text) => notify(&text),
                Action::Send(frame) => {
                    if let Some(connected) = &mut sender
                        && connected.send(&frame).await.is_err()
                    {
                        // The link task notices and reconnects.
                        sender = None;
                    }
                }
            }
        }
        if quit {
            break Ok(());
        }
    };
    link_task.abort();
    drop(terminal);
    outcome
}

/// Connects, starting the daemon if needed, and reconnects whenever the
/// connection is lost.
async fn keep_linked(options: Options, events: mpsc::UnboundedSender<LinkEvent>) {
    let mut last_start: Option<Instant> = None;
    loop {
        match connect(&options, &mut last_start).await {
            Ok(connection) => {
                let (sender, mut receiver, welcome) = connection.into_split();
                if events
                    .send(LinkEvent::Connected { sender, welcome })
                    .is_err()
                {
                    return;
                }
                let reason = loop {
                    match receiver.next().await {
                        Ok(Some(frame)) => {
                            if events.send(LinkEvent::Frame(Box::new(frame))).is_err() {
                                return;
                            }
                        }
                        Ok(None) => break "the daemon closed the connection".to_owned(),
                        Err(error) => break error.to_string(),
                    }
                };
                if events.send(LinkEvent::Lost(reason)).is_err() {
                    return;
                }
            }
            Err(error) => {
                if events.send(LinkEvent::Lost(error.to_string())).is_err() {
                    return;
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

/// Connects to the daemon, starting it if nothing answers and it was not
/// started in the last ten seconds.
async fn connect(
    options: &Options,
    last_start: &mut Option<Instant>,
) -> Result<Connection, ClientError> {
    match Connection::connect(&options.socket, CLIENT).await {
        Err(ClientError::Unreachable { .. }) => {}
        other => return other,
    }
    let Some(start) = &options.start_daemon else {
        return Connection::connect(&options.socket, CLIENT).await;
    };
    if last_start.is_none_or(|at| at.elapsed() > Duration::from_secs(10)) {
        *last_start = Some(Instant::now());
        if let Err(error) = start() {
            return Err(ClientError::Unreachable {
                path: options.socket.display().to_string(),
                source: error,
            });
        }
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match Connection::connect(&options.socket, CLIENT).await {
            Err(ClientError::Unreachable { .. }) if Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            other => return other,
        }
    }
}

fn terminal_size(terminal: &DefaultTerminal) -> std::io::Result<TermSize> {
    let size = terminal.size()?;
    Ok(area_size(size.width, size.height))
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
    enhanced_keys: bool,
}

impl HostTerminal {
    fn enter() -> std::io::Result<Self> {
        let terminal = ratatui::init();
        let mut out = stdout();
        // The mouse scrolls sessions back and picks lines; selecting text
        // in the person's terminal then takes a modifier (Option or Shift).
        execute!(
            out,
            EnableBracketedPaste,
            MouseReports(true),
            EnableFocusChange
        )?;
        let enhanced_keys = crossterm::terminal::supports_keyboard_enhancement().unwrap_or(false);
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
        Ok(Self {
            terminal,
            enhanced_keys,
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
/// (OSC 9: iTerm2, WezTerm, Ghostty, kitty, Windows Terminal) and rings its
/// bell, which the others show as a badge or a bounce. The text can carry
/// an issue's title, so control characters are taken out first: they could
/// end the sequence and send the terminal commands of their own.
fn notify(text: &str) {
    use std::io::Write;
    let mut out = stdout();
    let _ = write!(out, "\x1b]9;Muster: {}\x07\x07", printable(text));
    let _ = out.flush();
}

/// `text` without control characters.
fn printable(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).collect()
}

/// Asks the person's terminal for mouse presses, releases and the wheel,
/// and movement while a button is held, in SGR encoding; not for every
/// movement, as crossterm's `EnableMouseCapture` does, which would wake the
/// UI whenever the pointer crosses its window.
struct MouseReports(bool);

impl crossterm::Command for MouseReports {
    fn write_ansi(&self, out: &mut impl std::fmt::Write) -> std::fmt::Result {
        let switch = if self.0 { 'h' } else { 'l' };
        write!(
            out,
            "\x1b[?1000{switch}\x1b[?1002{switch}\x1b[?1006{switch}"
        )
    }
}

/// The wall-clock time, for how long things have taken.
fn wall_clock() -> muster_core::Timestamp {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_millis());
    muster_core::Timestamp::from_millis(i64::try_from(millis).unwrap_or(i64::MAX))
}

#[cfg(test)]
mod tests {
    #[test]
    fn notifications_carry_no_control_characters() {
        assert_eq!(
            super::printable("#7 evil\x1b]0;owned\x07title\u{9b}31m\n"),
            "#7 evil]0;ownedtitle31m"
        );
    }
}

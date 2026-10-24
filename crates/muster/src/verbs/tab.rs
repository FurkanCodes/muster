//! A session shown straight in the person's own terminal tab (`muster
//! attach`, `muster enter`): nothing of Muster's around it, every key but
//! the one that goes back reaching its program, and the tab back as it was
//! when the person leaves (`PRODUCT.md` §6b). To see the other sessions,
//! the person lists them in another tab (`muster list`).
//!
//! The tab keeps a stack of the sessions it showed. A session started from
//! inside one shown here (`muster claude` typed in a workspace's terminal)
//! takes its place, as the daemon tells this client to show it
//! (`Command::Switch`, then `ServerFrame::Reveal`); the back key (Ctrl-Q,
//! Settings › Keys) goes back to the session before it, and from the first
//! to the person's own shell. A session whose program ends goes back the
//! same way. Going back never stops a session.
//!
//! The tab says whether the person looks at it from the focus reports of
//! their terminal, so a tab in the background does not take what happens
//! in its session from *Needs you*; a terminal that sends none counts as
//! looked at.

use std::io::{IsTerminal, Read as _, Write as _};
use std::process::ExitCode;

use muster_client::{ClientError, FrameSender};
use muster_core::keys::{Action, Chord, Key, Keymap};
use muster_core::protocol::{
    ClientFrame, Command, ErrorCode, PixelSize, ServerFrame, Snapshot, TermSize,
};
use muster_core::{Paths, SessionId};
use tokio::sync::mpsc;

use super::targets;
use crate::commands::{self, shown};

/// What the person's terminal is put back to as a session leaves the tab,
/// or another takes its place: the main screen, no mouse, paste or focus
/// reports, no kitty keyboard flags, normal cursor keys, the cursor shown
/// and plain colours.
const PUT_BACK: &[u8] = b"\x1b[?1049l\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1006l\x1b[?1015l\
\x1b[?2004l\x1b[?1004l\x1b[<u\x1b[?1l\x1b>\x1b[?25h\x1b[0m";

/// Clears the screen, the cursor at the top; the person's scrollback
/// stays.
const CLEAR: &[u8] = b"\x1b[H\x1b[2J";

/// The cursor on the screen's last line, and a new line under it: the
/// person's prompt comes back under what the session showed last.
const BELOW: &[u8] = b"\x1b[999;1H\r\n";

/// Asks the person's terminal to say when it gains and loses focus.
const FOCUS_REPORTS: &[u8] = b"\x1b[?1004h";

/// What the person's terminal sends as it gains and loses focus.
const FOCUS_IN: &[u8] = b"\x1b[I";
const FOCUS_OUT: &[u8] = b"\x1b[O";

/// How long the start of an escape sequence that one read ended in waits
/// for its end in the next: a lone Esc is sent on after it.
const HELD_FOR: std::time::Duration = std::time::Duration::from_millis(30);

/// Shows `session` where the person is: inside a Muster session's
/// terminal, in its place, wherever it is shown; anywhere else, straight in
/// this tab.
pub(crate) async fn show(
    paths: &Paths,
    session: SessionId,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    if let Some(inside) = super::started_by().filter(|inside| *inside != session) {
        let mut connection = commands::connect(&paths.socket()).await?;
        match connection
            .command(Command::Switch {
                from: inside,
                to: session,
            })
            .await
        {
            Ok(_) => return Ok(ExitCode::SUCCESS),
            // Shown nowhere: this terminal is a script's.
            Err(ClientError::Daemon(error)) if error.code == ErrorCode::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    run(paths, session).await
}

/// Shows `first` in this tab until the person goes back from the first
/// session shown here, or it ends.
pub(crate) async fn run(
    paths: &Paths,
    first: SessionId,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return Err(
            "a session is shown in a terminal: `muster logs -f` follows one from a script".into(),
        );
    }
    let snapshot = commands::connect(&paths.socket())
        .await?
        .subscribe()
        .await?;
    refuse_ended(&snapshot, first)?;
    let back = back_keys(&snapshot);
    let connection = commands::connect(&paths.socket()).await?;
    let (sender, mut receiver, _) = connection.into_split();
    let mut tab = Tab {
        sender,
        back,
        stack: Vec::new(),
        looked_at: true,
        ended: None,
        held: Vec::new(),
    };
    let raw = Raw::enter()?;
    // Counted among the windows the person looks at, so a session started
    // from the one shown here comes here (`Command::Switch`).
    tab.send(ClientFrame::Looking { looking: true }).await?;
    tab.push(first).await?;
    let (keys, mut typed) = mpsc::channel::<Vec<u8>>(64);
    read_keys(keys)?;
    let mut resized =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::window_change())?;
    while let Some(&current) = tab.stack.last() {
        tokio::select! {
            frame = receiver.next() => {
                let Some(frame) = frame? else {
                    drop(raw);
                    return Err("the daemon stopped; your sessions resume when it starts again".into());
                };
                tab.on_frame(current, frame).await?;
            },
            keys = typed.recv() => match keys {
                Some(keys) => tab.on_keys(&keys).await?,
                // Standard input closed: the tab is gone.
                None => return Ok(ExitCode::SUCCESS),
            },
            _ = resized.recv() => tab.fit(current).await?,
            () = tokio::time::sleep(HELD_FOR), if !tab.held.is_empty() => tab.send_held().await?,
        }
    }
    drop(raw);
    say_left(paths, first, tab.ended).await;
    Ok(ExitCode::SUCCESS)
}

/// Refuses a session whose program has ended: there is nothing to show.
fn refuse_ended(snapshot: &Snapshot, session: SessionId) -> Result<(), String> {
    let Some(found) = snapshot.sessions.iter().find(|found| found.id == session) else {
        return Err("no such session (`muster list`)".to_owned());
    };
    if found.status.is_exited() {
        return Err(format!(
            "{} has ended: `muster restart {}` starts it again",
            shown(&found.title),
            found.id
        ));
    }
    Ok(())
}

/// The keys that go back, as the person set them (Settings › Keys), those
/// a tab can tell from typing; Muster's own where theirs do not read.
fn back_keys(snapshot: &Snapshot) -> Vec<Chord> {
    let keymap = Keymap::with(&snapshot.preferences.keys).unwrap_or_default();
    let set: Vec<Chord> = keymap
        .chords(Action::LeaveTab)
        .iter()
        .copied()
        .filter(|chord| chord.in_tab())
        .collect();
    if set.is_empty() {
        Keymap::default().chords(Action::LeaveTab).to_vec()
    } else {
        set
    }
}

/// Says where the person is back from, and how to go back.
async fn say_left(paths: &Paths, first: SessionId, ended: Option<SessionId>) {
    let snapshot = match commands::connect(&paths.socket()).await {
        Ok(mut connection) => connection.subscribe().await.ok(),
        Err(_) => None,
    };
    let name = |id: SessionId| {
        snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.sessions.iter().find(|found| found.id == id))
            .map_or_else(|| targets::short(id), |found| shown(&found.title))
    };
    match ended {
        Some(gone) if gone == first => println!("{} ended.", name(gone)),
        _ => println!(
            "Left {} ({}); it keeps running. `muster attach {}` goes back.",
            name(first),
            targets::short(first),
            targets::short(first)
        ),
    }
}

/// The person's terminal in raw mode, every key reaching this process as
/// typed, until dropped, which puts it back.
struct Raw;

impl Raw {
    fn enter() -> std::io::Result<Self> {
        crossterm::terminal::enable_raw_mode()?;
        Ok(Self)
    }
}

impl Drop for Raw {
    fn drop(&mut self) {
        let mut out = std::io::stdout();
        let _ = out.write_all(PUT_BACK);
        let _ = out.write_all(BELOW);
        let _ = out.flush();
        let _ = crossterm::terminal::disable_raw_mode();
    }
}

/// Reads what the person types on a thread of its own, so a read waiting
/// on a key never holds the process up as it ends.
fn read_keys(keys: mpsc::Sender<Vec<u8>>) -> std::io::Result<()> {
    std::thread::Builder::new()
        .name("muster-tab-keys".to_owned())
        .spawn(move || {
            let mut stdin = std::io::stdin();
            let mut buffer = [0_u8; 4096];
            loop {
                match stdin.read(&mut buffer) {
                    Ok(0) | Err(_) => return,
                    Ok(n) => {
                        if keys.blocking_send(buffer[..n].to_vec()).is_err() {
                            return;
                        }
                    }
                }
            }
        })?;
    Ok(())
}

/// The tab's sessions and its link to the daemon.
struct Tab {
    sender: FrameSender,
    /// The keys that go back.
    back: Vec<Chord>,
    /// The sessions shown here, the one shown now last.
    stack: Vec<SessionId>,
    /// Whether the person's terminal has focus, as it last said.
    looked_at: bool,
    /// The last session whose program ended while shown.
    ended: Option<SessionId>,
    /// The start of an escape sequence the last read ended in, kept for
    /// the read that ends it, so a focus report or a key that goes back is
    /// found when a read cuts it in two.
    held: Vec<u8>,
}

impl Tab {
    async fn send(&mut self, frame: ClientFrame) -> Result<(), ClientError> {
        self.sender.send(&frame).await
    }

    /// Shows `session` here, over what was shown: the one before is let
    /// go, and comes back as `session` goes.
    async fn push(&mut self, session: SessionId) -> Result<(), ClientError> {
        if let Some(&current) = self.stack.last() {
            self.send(ClientFrame::Detach { session: current }).await?;
        }
        // Coming back to one shown before goes back to it.
        if let Some(at) = self.stack.iter().position(|shown| *shown == session) {
            self.stack.truncate(at);
        }
        self.stack.push(session);
        self.attach(session).await
    }

    /// The session shown now goes; the one before it comes back.
    async fn pop(&mut self) -> Result<(), ClientError> {
        if let Some(gone) = self.stack.pop() {
            self.send(ClientFrame::Detach { session: gone }).await?;
        }
        if let Some(&back) = self.stack.last() {
            self.attach(back).await?;
        }
        Ok(())
    }

    async fn attach(&mut self, session: SessionId) -> Result<(), ClientError> {
        write_out(PUT_BACK);
        write_out(CLEAR);
        write_out(FOCUS_REPORTS);
        self.send(ClientFrame::Attach {
            session,
            from_seq: None,
            watching: true,
        })
        .await?;
        self.fit(session).await?;
        self.keys(session).await
    }

    /// The session's terminal takes the tab's size.
    async fn fit(&mut self, session: SessionId) -> Result<(), ClientError> {
        let Ok(window) = crossterm::terminal::window_size() else {
            return Ok(());
        };
        let size = TermSize {
            cols: window.columns,
            rows: window.rows,
        };
        let pixels = (window.width > 0 && window.height > 0).then_some(PixelSize {
            width: window.width,
            height: window.height,
        });
        self.send(ClientFrame::Resize {
            session,
            size,
            pixels,
        })
        .await
    }

    /// Tells the daemon whose terminal has the person's keys: the one
    /// shown here while the tab has focus.
    async fn keys(&mut self, session: SessionId) -> Result<(), ClientError> {
        let session = self.looked_at.then_some(session);
        self.send(ClientFrame::Keys { session }).await
    }

    /// Draws what the daemon sends of the session shown, and shows another
    /// where told to.
    async fn on_frame(
        &mut self,
        current: SessionId,
        frame: ServerFrame,
    ) -> Result<(), ClientError> {
        match frame {
            ServerFrame::Replay {
                session,
                data,
                reset,
                screen,
                ..
            } if session == current => {
                if reset {
                    write_out(CLEAR);
                }
                // The screen drawn whole, where the daemon gives it, rather
                // than everything that drew it.
                match screen {
                    Some(screen) => write_out(&screen),
                    None => write_out(&data),
                }
            }
            ServerFrame::Output { session, data, .. } if session == current => write_out(&data),
            ServerFrame::Detached { session, .. } if session == current => {
                self.ended = Some(session);
                self.pop().await?;
            }
            ServerFrame::Reveal { session } if session != current => self.push(session).await?,
            _ => {}
        }
        Ok(())
    }

    /// Sends what the person typed to the session shown, but the focus
    /// reports, which say whether they look at the tab, and the keys that
    /// go back, each to the session shown before: what follows one goes
    /// to that session.
    async fn on_keys(&mut self, keys: &[u8]) -> Result<(), ClientError> {
        let mut keys = [std::mem::take(&mut self.held).as_slice(), keys].concat();
        self.held = keys.split_off(unfinished_from(&keys));
        self.take_keys(&keys).await
    }

    /// Sends on what was held, its end never having come.
    async fn send_held(&mut self) -> Result<(), ClientError> {
        let held = std::mem::take(&mut self.held);
        self.take_keys(&held).await
    }

    async fn take_keys(&mut self, keys: &[u8]) -> Result<(), ClientError> {
        let (keys, focus) = take_focus(keys);
        if let Some(looking) = focus
            && looking != self.looked_at
        {
            self.looked_at = looking;
            self.send(ClientFrame::Looking { looking }).await?;
            if let Some(&current) = self.stack.last() {
                self.keys(current).await?;
            }
        }
        let mut rest = &keys[..];
        while let Some(&current) = self.stack.last() {
            let (typed, after) = match find_leave(rest, &self.back) {
                Some((at, end)) => (&rest[..at], Some(&rest[end..])),
                None => (rest, None),
            };
            if !typed.is_empty() {
                self.send(ClientFrame::Input {
                    session: current,
                    data: typed.to_vec(),
                })
                .await?;
            }
            let Some(after) = after else {
                break;
            };
            self.pop().await?;
            rest = after;
        }
        Ok(())
    }
}

/// Writes to the person's terminal at once.
fn write_out(bytes: &[u8]) {
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(bytes);
    let _ = out.flush();
}

/// Where an escape sequence `keys` ends in before its end begins: a lone
/// Esc, or a CSI with no final byte yet; `keys.len()` where none does.
fn unfinished_from(keys: &[u8]) -> usize {
    let Some(at) = keys.iter().rposition(|&byte| byte == 0x1b) else {
        return keys.len();
    };
    let unfinished = match &keys[at + 1..] {
        [] => true,
        [b'[', rest @ ..] => rest
            .iter()
            .all(|b| b.is_ascii_digit() || matches!(b, b';' | b':' | b'?')),
        _ => false,
    };
    if unfinished { at } else { keys.len() }
}

/// `keys` without the focus reports in them, and whether the last said the
/// tab has focus.
fn take_focus(keys: &[u8]) -> (Vec<u8>, Option<bool>) {
    let mut rest = Vec::with_capacity(keys.len());
    let mut focus = None;
    let mut at = 0;
    while at < keys.len() {
        if keys[at..].starts_with(FOCUS_IN) {
            focus = Some(true);
            at += FOCUS_IN.len();
        } else if keys[at..].starts_with(FOCUS_OUT) {
            focus = Some(false);
            at += FOCUS_OUT.len();
        } else {
            rest.push(keys[at]);
            at += 1;
        }
    }
    (rest, focus)
}

/// Where in `keys` the first of the `leave` keys begins and ends, however
/// the person's terminal sends it: as a control byte or an Esc before the
/// letter (legacy keys), in the kitty keyboard protocol (`CSI 113;5u`, a
/// release not counted), or as xterm's modified key (`CSI 27;5;113~`), as
/// the program shown may have asked of it.
fn find_leave(keys: &[u8], leave: &[Chord]) -> Option<(usize, usize)> {
    (0..keys.len()).find_map(|at| {
        leave
            .iter()
            .find_map(|chord| sent_as(&keys[at..], *chord))
            .map(|length| (at, at + length))
    })
}

/// How long `chord` is at the start of `keys`, if it is there, in any of
/// the ways a terminal sends it.
fn sent_as(keys: &[u8], chord: Chord) -> Option<usize> {
    let Key::Char(letter) = chord.key else {
        return None;
    };
    let letter = u8::try_from(letter).ok()?;
    let legacy: Vec<u8> = match (chord.ctrl, chord.alt) {
        (true, false) => vec![letter & 0x1f],
        (false, true) => vec![0x1b, letter],
        (true, true) => vec![0x1b, letter & 0x1f],
        (false, false) => return None,
    };
    if keys.starts_with(&legacy) {
        return Some(legacy.len());
    }
    let modifiers = 1 + if chord.alt { 2 } else { 0 } + if chord.ctrl { 4 } else { 0 };
    if let Some((code, mods, released, length)) = kitty_key(keys)
        && code == u32::from(letter)
        && mods == modifiers
        && !released
    {
        return Some(length);
    }
    let modified = format!("\x1b[27;{modifiers};{letter}~");
    keys.starts_with(modified.as_bytes())
        .then_some(modified.len())
}

/// The key a kitty keyboard protocol sequence at the start of `keys` sends:
/// its code, its modifiers, whether it is a release, and the sequence's
/// length (`CSI code[:alternates];modifiers[:event]u`).
fn kitty_key(keys: &[u8]) -> Option<(u32, u32, bool, usize)> {
    let body = keys.strip_prefix(b"\x1b[")?;
    let end = body
        .iter()
        .position(|b| !(b.is_ascii_digit() || *b == b';' || *b == b':'))?;
    if body[end] != b'u' {
        return None;
    }
    let text = std::str::from_utf8(&body[..end]).ok()?;
    let mut fields = text.split(';');
    let code = fields.next()?.split(':').next()?.parse().ok()?;
    let mut modifiers = fields.next().unwrap_or("1").split(':');
    let mods = modifiers.next()?.parse().ok()?;
    let released = modifiers.next() == Some("3");
    Some((code, mods, released, 2 + end + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chord(text: &str) -> Chord {
        Chord::parse(text).unwrap()
    }

    #[test]
    fn a_sequence_a_read_cuts_in_two_waits_for_its_end() {
        assert_eq!(unfinished_from(b"ls"), 2);
        assert_eq!(unfinished_from(b"ls\x1b"), 2);
        assert_eq!(unfinished_from(b"ls\x1b[11"), 2);
        assert_eq!(unfinished_from(b"ls\x1b[113;5"), 2);
        assert_eq!(unfinished_from(b"\x1b[I"), 3, "a focus report is whole");
        assert_eq!(unfinished_from(b"\x1b[113;5u"), 8);
        assert_eq!(unfinished_from(b"\x1bq"), 2, "Alt+q is whole");
    }

    #[test]
    fn the_leave_key_is_found_however_the_terminal_sends_it_prd_6b() {
        let ctrl_q = [chord("ctrl-q")];
        for sent in [
            &b"\x11"[..],
            b"\x1b[113;5u",
            b"\x1b[113;5:1u",
            b"\x1b[113:81;5u",
            b"\x1b[27;5;113~",
        ] {
            let mut keys = b"ls".to_vec();
            keys.extend_from_slice(sent);
            keys.extend_from_slice(b"pwd");
            let (at, end) = find_leave(&keys, &ctrl_q).unwrap();
            assert_eq!((at, &keys[end..]), (2, &b"pwd"[..]), "{sent:?}");
        }
        // Its release, another letter, or Ctrl with Shift is no leaving.
        for sent in [&b"\x1b[113;5:3u"[..], b"\x1b[119;5u", b"\x1b[113;6u", b"q"] {
            assert_eq!(find_leave(sent, &ctrl_q), None, "{sent:?}");
        }
        assert_eq!(find_leave(b"\x1bx", &[chord("alt-x")]), Some((0, 2)));
        assert_eq!(find_leave(b"\x1b[120;3u", &[chord("alt-x")]), Some((0, 8)));
    }

    #[test]
    fn focus_reports_say_whether_the_tab_is_looked_at_and_reach_no_program() {
        assert_eq!(take_focus(b"a\x1b[Ob"), (b"ab".to_vec(), Some(false)));
        assert_eq!(take_focus(b"\x1b[O\x1b[I"), (Vec::new(), Some(true)));
        assert_eq!(take_focus(b"\x1b[A"), (b"\x1b[A".to_vec(), None));
    }

    #[test]
    fn leaving_puts_the_terminal_back() {
        let text = String::from_utf8_lossy(PUT_BACK);
        for mode in [
            "?1049l", "?1000l", "?1006l", "?2004l", "?1004l", "<u", "?25h",
        ] {
            assert!(text.contains(mode), "{mode}");
        }
    }
}

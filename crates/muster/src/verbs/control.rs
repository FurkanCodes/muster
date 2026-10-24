//! A session's terminal driven from a shell (`PRODUCT.md` §6b), beside
//! `muster send`, which types words and presses Enter: `muster keys`
//! presses keys by name, `muster wait … --for` waits until it shows some text, and `muster
//! move` takes a terminal to another workspace. An agent may drive
//! sessions as the person could (`PRD-01`), but never answers what another
//! agent asks the person: that is theirs.

use std::process::ExitCode;
use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use muster_core::protocol::{Command, Reading, Reply, Snapshot, TerminalModes};
use muster_core::{Paths, Session, SessionStatus};

use super::send;
use super::targets::{self, workspace_name};
use super::workspaces;
use crate::commands::{self, shown, unexpected};

/// How often `wait --for` looks at the screen.
const LOOK_EVERY: Duration = Duration::from_millis(200);

/// `muster keys <s> KEY…`: keys pressed by name, each encoded as the
/// program asked its terminal to send keys; every one is read before any
/// is pressed.
pub(crate) async fn keys(
    paths: &Paths,
    session: &str,
    names: &[String],
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let mut connection = commands::connect(&paths.socket()).await?;
    let snapshot = connection.subscribe().await?;
    let session = targets::find_session(&snapshot, session)?;
    drivable(&snapshot, session)?;
    let id = session.id;
    connection.attach_unwatched(id, None).await?;
    let modes = tokio::time::timeout(send::MODES_WAIT, send::modes(&mut connection, id))
        .await
        .map_err(|_| "the session's terminal did not answer")??;
    let bytes: Vec<Vec<u8>> = names
        .iter()
        .map(|name| key_bytes(name, modes))
        .collect::<Result<_, _>>()?;
    for key in bytes {
        connection.input(id, &key).await?;
    }
    connection.detach(id).await?;
    Ok(ExitCode::SUCCESS)
}

/// Refuses a session whose program has ended; and, run inside an agent's
/// session, one that asks the person something: its answer is theirs.
fn drivable(snapshot: &Snapshot, session: &Session) -> Result<(), String> {
    if session.status.is_exited() {
        return Err(format!(
            "{} has ended: `muster restart` starts it again",
            shown(&session.title)
        ));
    }
    let by_agent = super::started_by()
        .and_then(|inside| snapshot.sessions.iter().find(|found| found.id == inside))
        .is_some_and(|inside| inside.kind().is_agent());
    if by_agent && matches!(session.status, SessionStatus::NeedsYou { .. }) {
        return Err(format!(
            "{} asks the person something, and an agent does not answer for them",
            shown(&session.title)
        ));
    }
    Ok(())
}

/// The bytes a terminal sends for the key `name` (`enter`, `esc`, `tab`,
/// `backspace`, `up`, `f5`, `ctrl+c`, `alt+x`, `shift+tab`, or one
/// character), as `modes` ask, encoded as the UI encodes what the person
/// types (`muster_tui::keys`).
pub(crate) fn key_bytes(name: &str, modes: TerminalModes) -> Result<Vec<u8>, String> {
    named_key(name)
        .and_then(|key| muster_tui::keys::encode_key(&key, modes))
        .ok_or_else(|| not_a_key(name))
}

/// The key `name` names, or `None`.
fn named_key(name: &str) -> Option<KeyEvent> {
    let lower = name.to_lowercase();
    for (prefixes, modifier) in [
        (["ctrl+", "ctrl-", "c-"], KeyModifiers::CONTROL),
        (["alt+", "alt-", "m-"], KeyModifiers::ALT),
    ] {
        if let Some(rest) = prefixes
            .iter()
            .find_map(|prefix| lower.strip_prefix(prefix))
        {
            let mut chars = rest.chars();
            return match (chars.next(), chars.next()) {
                (Some(c), None) if c.is_ascii_lowercase() => {
                    Some(KeyEvent::new(KeyCode::Char(c), modifier))
                }
                _ => None,
            };
        }
    }
    let code = match lower.as_str() {
        "enter" | "return" => KeyCode::Enter,
        "tab" => KeyCode::Tab,
        "shift+tab" | "shift-tab" | "backtab" => {
            return Some(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));
        }
        "esc" | "escape" => KeyCode::Esc,
        "backspace" => KeyCode::Backspace,
        "delete" | "del" => KeyCode::Delete,
        "space" => KeyCode::Char(' '),
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "right" => KeyCode::Right,
        "left" => KeyCode::Left,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        function if function.len() > 1 && function.starts_with('f') => {
            match function[1..].parse::<u8>() {
                Ok(n @ 1..=12) => KeyCode::F(n),
                _ => return None,
            }
        }
        _ => match name.chars().collect::<Vec<_>>().as_slice() {
            [c] if !c.is_control() => KeyCode::Char(*c),
            _ => return None,
        },
    };
    Some(KeyEvent::new(code, KeyModifiers::NONE))
}

fn not_a_key(name: &str) -> String {
    format!(
        "{name:?} is not a key: write enter, esc, tab, backspace, delete, space, up, down, left, \
         right, home, end, pageup, pagedown, f1 to f12, ctrl+c, alt+x, shift+tab, or one character"
    )
}

/// `muster wait <s> --for TEXT`: until the session shows TEXT, exiting 0;
/// 4 if its program ends first, 124 out of time. What already shows counts.
pub(crate) async fn wait_for_text(
    paths: &Paths,
    session: &str,
    wanted: &str,
    timeout: Option<Duration>,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let mut connection = commands::connect(&paths.socket()).await?;
    let snapshot = connection.subscribe().await?;
    let id = targets::find_session(&snapshot, session)?.id;
    let deadline = timeout.map(|timeout| crate::instant_now() + timeout);
    loop {
        let reply = connection
            .command(Command::ReadSession {
                session: id,
                what: Reading::Screen,
            })
            .await?;
        let Reply::SessionText { text } = reply else {
            return Err(unexpected(&reply));
        };
        let Some(text) = text else {
            eprintln!("muster: its program ended first");
            return Ok(ExitCode::from(4));
        };
        if let Some(line) = text.lines().find(|line| line.contains(wanted)) {
            println!("{}", muster_core::text::one_line(line.trim_end()));
            return Ok(ExitCode::SUCCESS);
        }
        if deadline.is_some_and(|deadline| crate::instant_now() >= deadline) {
            eprintln!("muster: out of time; it shows no {wanted:?}");
            return Ok(ExitCode::from(super::wait::TIMED_OUT));
        }
        tokio::time::sleep(LOOK_EVERY).await;
    }
}

/// `muster move <s> --to WS`: a terminal taken to another workspace: a new
/// one started there, under the name the person gave the old, shown where
/// the old was shown, and the old one closed. What ran in it ends, as it
/// would in any shell that changes folder; an agent is never moved, as
/// its work is in its workspace.
pub(crate) async fn move_terminal(
    paths: &Paths,
    session: &str,
    to: &str,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let mut connection = commands::connect(&paths.socket()).await?;
    let snapshot = connection.subscribe().await?;
    let old = targets::find_session(&snapshot, session)?;
    if !old.kind().is_shell() {
        return Err(format!(
            "{} is an agent, whose work is in its workspace: start another where you want one \
             (`muster claude --in {to}`)",
            shown(&old.title)
        )
        .into());
    }
    let from_project = snapshot
        .workspaces
        .iter()
        .find(|workspace| workspace.id == old.workspace)
        .map(|workspace| workspace.project);
    let target = targets::find_workspace(&snapshot, to, from_project)?;
    if target.id == old.workspace {
        return Err(format!("it is in {to} already").into());
    }
    let (old_id, title) = (old.id, old.title.clone());
    let place = workspace_name(&snapshot, target);
    let new = workspaces::start_terminal(&mut connection, target.id, Some(title)).await?;
    // A tab or window showing the old one shows the new one now.
    let _ = connection
        .command(Command::Switch {
            from: old_id,
            to: new,
        })
        .await;
    connection
        .command(Command::StopSession { session: old_id })
        .await?;
    workspaces::stopped(&mut connection, &[old_id]).await?;
    connection
        .command(Command::RemoveSession { session: old_id })
        .await?;
    let snapshot = connection.subscribe().await?;
    let number = snapshot
        .sessions
        .iter()
        .find(|found| found.id == new)
        .and_then(|found| targets::number_of(&snapshot, found))
        .unwrap_or_default();
    println!(
        "Moved it to {} ({}/{number}).",
        shown(&place),
        shown(&place)
    );
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn modes(kitty_flags: u8, application_cursor: bool) -> TerminalModes {
        TerminalModes {
            kitty_flags,
            application_cursor,
            ..TerminalModes::default()
        }
    }

    #[test]
    fn keys_are_named_and_sent_as_the_program_asked_prd_6b() {
        let legacy = modes(0, false);
        assert_eq!(key_bytes("enter", legacy).unwrap(), b"\r");
        assert_eq!(key_bytes("ctrl+c", legacy).unwrap(), b"\x03");
        assert_eq!(key_bytes("C-c", legacy).unwrap(), b"\x03");
        assert_eq!(key_bytes("alt+x", legacy).unwrap(), b"\x1bx");
        assert_eq!(key_bytes("up", legacy).unwrap(), b"\x1b[A");
        assert_eq!(key_bytes("up", modes(0, true)).unwrap(), b"\x1bOA");
        assert_eq!(key_bytes("f1", legacy).unwrap(), b"\x1bOP");
        assert_eq!(key_bytes("f5", legacy).unwrap(), b"\x1b[15~");
        assert_eq!(key_bytes("1", legacy).unwrap(), b"1");
        let kitty = modes(1, false);
        assert_eq!(key_bytes("esc", kitty).unwrap(), b"\x1b[27u");
        assert_eq!(key_bytes("ctrl+c", kitty).unwrap(), b"\x1b[99;5u");
        // Kitty's form only for what is ambiguous, unless every key was asked.
        assert_eq!(key_bytes("enter", kitty).unwrap(), b"\r");
        assert_eq!(
            key_bytes("enter", modes(1 | 8, false)).unwrap(),
            b"\x1b[13u"
        );
        for not in ["ctrl+", "ctrl+ab", "f13", "hello", "\u{7}"] {
            assert!(key_bytes(not, legacy).is_err(), "{not:?}");
        }
    }
}

//! `muster send` and `muster prompt`: words typed into a session's
//! terminal as a paste, then Enter, as the person would.

use std::io::Read as _;
use std::process::ExitCode;
use std::time::Duration;

use muster_client::Connection;
use muster_core::protocol::{ServerFrame, TerminalModes};
use muster_core::{Paths, Session, SessionId, SessionStatus, WaitReason};

use super::targets;
use super::wait::{self, ANY_STOP, Waiting};
use crate::commands::{self, shown};

/// How long after the paste Enter is pressed: an agent that reads a fast
/// paste and Enter as one paste would take the Enter as a new line in it.
const BEFORE_ENTER: Duration = Duration::from_millis(150);
/// How long the terminal has to say how it takes keys.
pub(crate) const MODES_WAIT: Duration = Duration::from_secs(5);

pub(crate) async fn send(
    paths: &Paths,
    session: &str,
    words: &[String],
    enter: bool,
    force: bool,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let text = text(words)?;
    let mut connection = commands::connect(&paths.socket()).await?;
    let snapshot = connection.subscribe().await?;
    let session = targets::find_session(&snapshot, session)?;
    sendable(session, force)?;
    type_into(&mut connection, session.id, &text, enter).await?;
    println!(
        "Sent to {} ({}).",
        shown(&session.title),
        targets::short(session.id)
    );
    Ok(ExitCode::SUCCESS)
}

pub(crate) async fn prompt(
    paths: &Paths,
    session: &str,
    words: &[String],
    timeout: Option<Duration>,
    force: bool,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let text = text(words)?;
    let mut connection = commands::connect(&paths.socket()).await?;
    // Subscribed before it is sent, so no change after it goes unseen.
    let snapshot = connection.subscribe().await?;
    let session = targets::find_session(&snapshot, session)?;
    if !session.kind().is_agent() {
        return Err(format!(
            "{} is no agent's chat; `muster send` types into it",
            shown(&session.title)
        )
        .into());
    }
    sendable(session, force)?;
    let id = session.id;
    type_into(&mut connection, id, &text, true).await?;
    let waiting = Waiting {
        until: ANY_STOP.to_vec(),
        any: false,
        timeout,
        begin_within: Some(super::BEGIN_WITHIN),
    };
    let statuses = super::statuses(&snapshot);
    let waited = wait::follow(&mut connection, &[id], statuses, &waiting).await?;
    super::tell(&mut connection, &snapshot, &waited).await?;
    Ok(waited.code())
}

/// The words as one text; `-` reads it from standard input.
fn text(words: &[String]) -> Result<String, Box<dyn std::error::Error>> {
    let text = if words == ["-"] {
        let mut text = String::new();
        std::io::stdin().read_to_string(&mut text)?;
        text.trim_end().to_owned()
    } else {
        words.join(" ")
    };
    if text.trim().is_empty() {
        return Err("nothing to send: give the words, or `-` to read them".into());
    }
    Ok(text)
}

/// Refuses a session whose program has ended, or whose agent asks for a
/// permission, where the words would be its answer, unless `force`.
fn sendable(session: &Session, force: bool) -> Result<(), String> {
    match session.status {
        SessionStatus::Exited { .. } => Err(format!(
            "{} has ended: `muster restart` starts it again",
            shown(&session.title)
        )),
        SessionStatus::NeedsYou {
            reason: WaitReason::Permission | WaitReason::Startup,
        } if !force => Err(format!(
            "{} asks you something first, which this would answer: `muster attach {}` to \
             answer it, or --force",
            shown(&session.title),
            targets::short(session.id)
        )),
        _ => Ok(()),
    }
}

/// Types `text` into `session`'s terminal as a paste, as its program takes
/// one, then Enter if `enter`.
pub(crate) async fn type_into(
    connection: &mut Connection,
    session: SessionId,
    text: &str,
    enter: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    connection.attach_unwatched(session, None).await?;
    let modes = tokio::time::timeout(MODES_WAIT, modes(connection, session))
        .await
        .map_err(|_| "the session's terminal did not answer")??;
    connection.input(session, &paste(text, modes)).await?;
    if enter {
        tokio::time::sleep(BEFORE_ENTER).await;
        connection.input(session, &enter_key(modes)).await?;
    }
    connection.detach(session).await?;
    Ok(())
}

/// How `session`'s program takes keys, as its stream says after its
/// replay.
pub(crate) async fn modes(
    connection: &mut Connection,
    session: SessionId,
) -> Result<TerminalModes, Box<dyn std::error::Error>> {
    loop {
        match connection.next_event().await? {
            Some(ServerFrame::Modes { session: of, modes }) if of == session => return Ok(modes),
            Some(ServerFrame::Detached { session: of, .. }) if of == session => {
                return Err("the session's program is not running".into());
            }
            Some(_) => {}
            None => return Err("the daemon closed the connection".into()),
        }
    }
}

/// The bytes for pasting `text`, as the UI pastes (`muster_tui::keys`).
pub(crate) fn paste(text: &str, modes: TerminalModes) -> Vec<u8> {
    muster_tui::keys::encode_paste(text, modes)
}

/// Enter, as the UI sends it (`muster_tui::keys`).
pub(crate) fn enter_key(modes: TerminalModes) -> Vec<u8> {
    let enter = crossterm::event::KeyEvent::from(crossterm::event::KeyCode::Enter);
    muster_tui::keys::encode_key(&enter, modes).unwrap_or_else(|| b"\r".to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn modes(bracketed_paste: bool, kitty_flags: u8) -> TerminalModes {
        TerminalModes {
            kitty_flags,
            bracketed_paste,
            ..TerminalModes::default()
        }
    }

    #[test]
    fn words_are_pasted_as_the_program_asked_and_cannot_end_the_paste() {
        assert_eq!(paste("a\nb", modes(false, 0)), b"a\rb".to_vec());
        assert_eq!(
            paste("fix\x1b[201~it", modes(true, 0)),
            b"\x1b[200~fixit\x1b[201~".to_vec()
        );
        assert_eq!(enter_key(modes(true, 0)), b"\r");
        assert_eq!(enter_key(modes(true, 1)), b"\r");
        assert_eq!(enter_key(modes(true, 1 | 8)), b"\x1b[13u");
    }

    #[test]
    fn an_ended_session_or_one_asking_is_not_typed_into_unless_forced() {
        let mut session = Session {
            id: SessionId::new(),
            workspace: muster_core::WorkspaceId::new(),
            harness: "claude".to_owned(),
            title: "Fix".to_owned(),
            status: SessionStatus::NeedsYou {
                reason: WaitReason::Permission,
            },
            unseen: false,
            resume_id: None,
            created_at: muster_core::Timestamp::from_millis(0),
            ended_at: None,
            status_since: muster_core::Timestamp::from_millis(0),
            activity: None,
            spend: None,
            tidy: None,
            beside: None,
            started_by: None,
            agent: None,
        };
        assert!(sendable(&session, false).unwrap_err().contains("--force"));
        assert_eq!(sendable(&session, true), Ok(()));
        session.status = SessionStatus::Finished;
        assert_eq!(sendable(&session, false), Ok(()));
        session.status = SessionStatus::Exited { success: true };
        assert!(sendable(&session, true).is_err());
    }
}

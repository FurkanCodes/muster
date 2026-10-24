//! `muster logs`: what a session shows, as text drawn by the daemon at its
//! program's own size (a full-screen agent's output makes no sense as
//! bytes), its agent's last answer, or its output as it comes. It never
//! watches the session, so what happens there stays news (`PRD-01`'s *Needs
//! you* is the person's).

use std::io::Write as _;
use std::process::ExitCode;

use muster_core::protocol::{Command, DetachReason, Reading, Reply, ServerFrame};
use muster_core::{Paths, SessionId};

use super::targets;
use crate::commands::{self, unexpected};

pub(crate) async fn run(
    paths: &Paths,
    session: &str,
    reply: bool,
    follow: bool,
    lines: Option<usize>,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let mut connection = commands::connect(&paths.socket()).await?;
    let snapshot = connection.subscribe().await?;
    let id = targets::find_session(&snapshot, session)?.id;
    let what = if reply {
        Reading::LastReply
    } else {
        Reading::Screen
    };
    let answer = connection
        .command(Command::ReadSession { session: id, what })
        .await?;
    let Reply::SessionText { text } = answer else {
        return Err(unexpected(&answer));
    };
    match text {
        Some(text) => println!("{}", muster_core::text::multi_line(&last(&text, lines))),
        None if reply => eprintln!("muster: no answer yet: its agent has ended no turn here"),
        None => eprintln!("muster: its terminal is no longer held"),
    }
    if follow {
        stream(&mut connection, id).await?;
    }
    Ok(ExitCode::SUCCESS)
}

/// The last `lines` lines of `text`, or all of it.
fn last(text: &str, lines: Option<usize>) -> String {
    let Some(lines) = lines else {
        return text.to_owned();
    };
    let all: Vec<&str> = text.lines().collect();
    all[all.len().saturating_sub(lines)..].join("\n")
}

/// Writes the session's output from now on as it comes, until its program
/// ends: what it already showed was printed above.
async fn stream(
    connection: &mut muster_client::Connection,
    session: SessionId,
) -> Result<(), Box<dyn std::error::Error>> {
    connection.attach_unwatched(session, None).await?;
    let mut out = std::io::stdout();
    let mut position = crate::output::Position::default();
    let mut past_replay = false;
    loop {
        match connection.next_event().await? {
            Some(ServerFrame::Replay {
                session: of,
                seq,
                data,
                ..
            }) if of == session => {
                // History: shown above. Only where the stream starts again
                // later (a new program) is it new.
                let fresh = position.take(seq, &data);
                if past_replay {
                    out.write_all(fresh.data)?;
                    out.flush()?;
                }
                past_replay = true;
            }
            Some(ServerFrame::Output {
                session: of,
                seq,
                data,
            }) if of == session => {
                let fresh = position.take(seq, &data);
                out.write_all(fresh.data)?;
                out.flush()?;
            }
            Some(ServerFrame::Detached {
                session: of,
                reason,
            }) if of == session => {
                if reason == DetachReason::NotRunning {
                    return Err("its program is not running".into());
                }
                return Ok(());
            }
            Some(_) => {}
            None => return Err("the daemon closed the connection".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_last_lines_are_the_end_of_what_it_shows() {
        assert_eq!(last("a\nb\nc", Some(2)), "b\nc");
        assert_eq!(last("a\nb", Some(9)), "a\nb");
        assert_eq!(last("a\nb", None), "a\nb");
    }
}

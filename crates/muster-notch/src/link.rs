//! The notch's link to the daemon: a client like any other (`ARC-22`). It
//! subscribes to the state, works out the board, and hands it to the
//! screen; it asks for the sessions the person picks to be shown in a
//! Muster window. It never starts the daemon: with none, there is nothing
//! to show, and once the daemon has been gone a while the notch goes too.

use std::path::Path;
use std::time::Duration;

use muster_client::{ClientError, Connection};
use muster_core::protocol::{ClientFrame, Command, ServerFrame, Snapshot};
use muster_core::settings::NOTCH_PROTOCOL;
use muster_core::{SessionId, Timestamp};
use tokio::sync::mpsc;
use tokio::time::{Instant, MissedTickBehavior};

use crate::board::Board;
use crate::mirror;

/// How the notch says who it is to the daemon.
const CLIENT: &str = concat!("muster-notch ", env!("CARGO_PKG_VERSION"));

/// How long each step of the motion is shown, as in the UI.
pub(crate) const STEP: Duration = Duration::from_millis(100);

/// How long the notch waits for a daemon that went away: a restart in
/// place is back within a second, a stop is for good.
const PATIENCE: Duration = Duration::from_secs(30);

/// What the link hands the screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ToScreen {
    /// What to show, at the motion's `step`.
    Board { board: Board, step: u64 },
    /// A word for the person under the list: why a session could not be
    /// shown.
    Note(String),
}

/// Why the link ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Ended {
    /// The person turned the notch off.
    TurnedOff,
    /// The daemon has been gone too long, or speaks no notch.
    NoDaemon(String),
    /// The daemon is another version of Muster: the notch should become
    /// it too.
    NewVersion(String),
}

/// Keeps the screen told until the notch should end. `picks` are the
/// sessions the person picks in the notch; `same_version` says whether the
/// daemon's version is one this notch need not move onto.
pub(crate) async fn run(
    socket: &Path,
    mut picks: mpsc::Receiver<SessionId>,
    same_version: impl Fn(&str) -> bool,
    screen: impl Fn(ToScreen),
) -> Ended {
    let mut gone_since: Option<Instant> = None;
    loop {
        match Connection::connect(socket, CLIENT).await {
            Ok(connection) => {
                gone_since = None;
                let welcome = connection.welcome().clone();
                if welcome.version < NOTCH_PROTOCOL {
                    return Ended::NoDaemon(format!(
                        "the daemon (muster {}) knows no notch",
                        welcome.daemon_version
                    ));
                }
                if !same_version(&welcome.daemon_version) {
                    return Ended::NewVersion(welcome.daemon_version);
                }
                if let Some(ended) = serve(connection, &mut picks, &screen).await {
                    return ended;
                }
            }
            Err(ClientError::Incompatible(incompatible)) => {
                return Ended::NoDaemon(format!(
                    "the daemon (muster {}) speaks another protocol",
                    incompatible.daemon_version
                ));
            }
            Err(_) => {}
        }
        // Gone: nothing to show meanwhile.
        screen(ToScreen::Board {
            board: Board::hidden(),
            step: 0,
        });
        let since = *gone_since.get_or_insert_with(Instant::now);
        if since.elapsed() > PATIENCE {
            return Ended::NoDaemon("the daemon is gone".to_owned());
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

/// Follows the daemon's state until the connection goes (`None`) or the
/// notch should end.
async fn serve(
    connection: Connection,
    picks: &mut mpsc::Receiver<SessionId>,
    screen: &impl Fn(ToScreen),
) -> Option<Ended> {
    let (mut sender, mut receiver, _) = connection.into_split();
    sender.send(&ClientFrame::Subscribe).await.ok()?;
    let mut state = Snapshot::default();
    let mut loaded = false;
    let mut next_req: u64 = 1;
    let mut steps = tokio::time::interval(STEP);
    steps.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut step: u64 = 0;
    let mut shown: Option<Board> = None;
    loop {
        let moving = shown.as_ref().is_some_and(|board| board.shown);
        tokio::select! {
            frame = receiver.next() => match frame {
                Ok(Some(ServerFrame::Snapshot(snapshot))) => {
                    state = snapshot;
                    loaded = true;
                }
                Ok(Some(ServerFrame::Delta(delta))) => mirror::apply(&mut state, delta),
                Ok(Some(ServerFrame::Reply { outcome: Err(error), .. })) => {
                    screen(ToScreen::Note(error.message));
                }
                Ok(Some(_)) => {}
                Ok(None) | Err(_) => return None,
            },
            Some(session) = picks.recv() => {
                let frame = ClientFrame::Command {
                    req_id: next_req,
                    command: Command::Reveal { session },
                };
                next_req += 1;
                sender.send(&frame).await.ok()?;
            }
            _ = steps.tick(), if moving => step = step.wrapping_add(1),
        }
        if !loaded {
            continue;
        }
        if !state.preferences.notch {
            return Some(Ended::TurnedOff);
        }
        let board = Board::of(&state, true, wall_clock());
        // Told while it moves, and otherwise only of a change.
        if board.shown || shown.as_ref() != Some(&board) {
            screen(ToScreen::Board {
                board: board.clone(),
                step,
            });
        }
        shown = Some(board);
    }
}

/// The time now, for how long sessions have run or waited: the notch's
/// one wall clock (`ARC-03`).
#[expect(
    clippy::disallowed_methods,
    reason = "ARC-03: the notch's one wall clock"
)]
fn wall_clock() -> Timestamp {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| {
            i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
        });
    Timestamp::from_millis(millis)
}

//! The notch's link to the daemon: a client like any other (`ARC-22`). It
//! subscribes to the state, works out the board, and hands it to the
//! screen; it asks for the sessions the person picks to be shown in a
//! Muster window. It tells the screen what newly needs the person while
//! they look away, for a notification of the Mac's own, and the daemon
//! whether it can notify (`news`). It never starts the daemon: with none,
//! there is nothing to show, and once the daemon has been gone a while
//! the notch goes too.

use std::path::Path;
use std::time::Duration;

use muster_client::{ClientError, Connection};
use muster_core::protocol::{ClientFrame, Command, ServerFrame, Snapshot};
use muster_core::settings::{NOTCH_PROTOCOL, NOTIFIER_PROTOCOL};
use muster_core::{SessionId, Timestamp};
use tokio::sync::mpsc;
use tokio::time::Instant;

use crate::board::Board;
use crate::mirror;
use crate::news::{News, Posted, posts};

/// How the notch says who it is to the daemon.
const CLIENT: &str = concat!("muster-notch ", env!("CARGO_PKG_VERSION"));

/// How long the notch waits for a daemon that went away: a restart in
/// place is back within a second, a stop is for good.
const PATIENCE: Duration = Duration::from_secs(30);

/// What the link hands the screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ToScreen {
    /// What to show. Motion is owned by AppKit, not the link.
    Board(Board),
    /// A word for the person under the list: why a session could not be
    /// shown.
    Note(String),
    /// A notification of the Mac's own.
    Notify(Posted),
}

/// What the screen hands the link.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FromScreen {
    /// Times are visible only in an open list or a peek.
    Expanded(bool),
    /// The person picked a session, in the notch or a notification: show
    /// it in a Muster window.
    Picked(SessionId),
    /// Whether notifications of the Mac's own can be shown: the person
    /// allowed them, or not, or the program is not Muster's app.
    CanNotify(bool),
}

/// Why the link ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Ended {
    /// The person turned the notch off, and notifications too, or the
    /// Mac shows none.
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
    mut picks: mpsc::Receiver<FromScreen>,
    same_version: impl Fn(&str) -> bool,
    screen: impl Fn(ToScreen),
) -> Ended {
    let mut gone_since: Option<Instant> = None;
    // Whether the Mac shows notifications here: unknown until it says.
    let mut can_notify: Option<bool> = None;
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
                let serving = serve(connection, &mut picks, &mut can_notify, &screen);
                if let Some(ended) = serving.await {
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
        screen(ToScreen::Board(Board::hidden()));
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
    picks: &mut mpsc::Receiver<FromScreen>,
    can_notify: &mut Option<bool>,
    screen: &impl Fn(ToScreen),
) -> Option<Ended> {
    let notifies = connection.welcome().version >= NOTIFIER_PROTOCOL;
    let (mut sender, mut receiver, _) = connection.into_split();
    sender.send(&ClientFrame::Subscribe).await.ok()?;
    let mut news = News::default();
    // What the daemon was last told of notifying, on this connection.
    let mut told_ready: Option<bool> = None;
    let mut state = Snapshot::default();
    let mut loaded = false;
    let mut expanded = false;
    let mut next_req: u64 = 1;
    let mut shown: Option<Board> = None;
    loop {
        let now = Instant::now();
        let time = shown
            .as_ref()
            .filter(|_| expanded)
            .and_then(|board| board.next_change(&state, wall_clock()))
            .map(|left| now + left);
        let wake = [time, news.next_due()].into_iter().flatten().min();
        tokio::select! {
            frame = receiver.next() => match frame {
                Ok(Some(ServerFrame::Snapshot(snapshot))) => {
                    state = snapshot;
                    news.known(&state);
                    loaded = true;
                }
                Ok(Some(ServerFrame::Delta(delta))) => {
                    mirror::apply(&mut state, delta);
                    let told = news.changed(&state, Instant::now());
                    notify(&state, &told, *can_notify, screen);
                }
                Ok(Some(ServerFrame::Reply { outcome: Err(error), .. })) => {
                    screen(ToScreen::Note(error.message));
                }
                Ok(Some(_)) => {}
                Ok(None) | Err(_) => return None,
            },
            Some(said) = picks.recv() => match said {
                FromScreen::Expanded(open) => expanded = open,
                FromScreen::Picked(session) => {
                    let frame = ClientFrame::Command {
                        req_id: next_req,
                        command: Command::Reveal { session },
                    };
                    next_req += 1;
                    sender.send(&frame).await.ok()?;
                }
                FromScreen::CanNotify(can) => {
                    if *can_notify != Some(can) {
                        eprintln!(
                            "muster notch: notifications of the Mac's own {}",
                            if can { "allowed" } else { "not allowed" }
                        );
                    }
                    *can_notify = Some(can);
                }
            },
            () = sleep_until(wake) => {
                let told = news.due(&state, Instant::now());
                notify(&state, &told, *can_notify, screen);
            }
        }
        if !loaded {
            continue;
        }
        let ready = state.preferences.notifications && *can_notify == Some(true);
        if notifies && told_ready != Some(ready) {
            sender.send(&ClientFrame::Notifier { ready }).await.ok()?;
            told_ready = Some(ready);
        }
        // Wanted while the notch is on, or notifications are and the Mac
        // may show them.
        let notifying = state.preferences.notifications && *can_notify != Some(false);
        if !state.preferences.notch && !notifying {
            return Some(Ended::TurnedOff);
        }
        let board = Board::of(&state, true, wall_clock());
        if shown.as_ref() != Some(&board) {
            screen(ToScreen::Board(board.clone()));
        }
        shown = Some(board);
    }
}

async fn sleep_until(wake: Option<Instant>) {
    match wake {
        Some(at) => tokio::time::sleep_until(at).await,
        None => std::future::pending().await,
    }
}

/// Has the screen notify the person of each of `told`, while they look
/// away from every Muster window, want to be told, and the Mac shows
/// notifications.
fn notify(
    state: &Snapshot,
    told: &[SessionId],
    can_notify: Option<bool>,
    screen: &impl Fn(ToScreen),
) {
    if !state.preferences.notifications || state.looked_at || can_notify != Some(true) {
        return;
    }
    for posted in posts(state, told) {
        screen(ToScreen::Notify(posted));
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

#![expect(
    clippy::disallowed_methods,
    reason = "tests stand in for the connection's writer; ARC-14 binds the app's own code"
)]

use std::sync::Mutex;

use muster_core::protocol::{Delta, ErrorBody, ErrorCode, Snapshot};
use muster_core::{Project, Session, Timestamp, WaitReason, Workspace};
use muster_model::fixtures::{project, session, worktree};

use super::*;
use crate::contract::Patch;

/// What the webview was sent.
#[derive(Clone, Default)]
struct Told(Arc<Mutex<Vec<StateMessage>>>);

impl Told {
    fn take(&self) -> Vec<StateMessage> {
        std::mem::take(&mut *self.0.lock().unwrap())
    }

    fn patches(&self) -> Vec<Patch> {
        self.take()
            .into_iter()
            .filter_map(|message| match message {
                StateMessage::Patch { patch } => Some(*patch),
                _ => None,
            })
            .collect()
    }
}

impl StateSink for Told {
    fn send(&self, message: StateMessage) {
        self.0.lock().unwrap().push(message);
    }
}

/// An actor whose webview is `told`, connected to a daemon of protocol
/// `version` whose frames come out of the receiver.
fn connected(version: u32) -> (Actor, Told, mpsc::UnboundedReceiver<ClientFrame>) {
    let mut actor = Actor::new(Arc::new(AtomicBool::new(false)));
    let told = Told::default();
    actor.on_request(Request::Open(Box::new(told.clone())));
    let (outgoing, frames) = mpsc::unbounded_channel();
    actor.on_event(LinkEvent::Connected {
        outgoing,
        welcome: Welcome {
            version,
            daemon_pid: 1,
            daemon_version: "0.4.1".to_owned(),
        },
    });
    (actor, told, frames)
}

fn sent(frames: &mut mpsc::UnboundedReceiver<ClientFrame>) -> Vec<ClientFrame> {
    std::iter::from_fn(|| frames.try_recv().ok()).collect()
}

fn snapshot_of(sessions: Vec<Session>, workspace: Workspace, project: Project) -> Snapshot {
    Snapshot {
        projects: vec![project],
        workspaces: vec![workspace],
        sessions,
        ..Snapshot::default()
    }
}

fn one_session() -> (Snapshot, Session) {
    let app = project("app");
    let tree = worktree(app.id, "work");
    let one = session(tree.id, "one", 3);
    (snapshot_of(vec![one.clone()], tree, app), one)
}

#[tokio::test(start_paused = true)]
async fn connecting_subscribes_and_says_how_the_person_looks() {
    let (actor, told, mut frames) = connected(44);
    assert!(actor.connected.load(Ordering::Relaxed));
    assert_eq!(
        sent(&mut frames),
        [
            ClientFrame::Subscribe,
            ClientFrame::Looking { looking: true },
            ClientFrame::Keys { session: None },
        ]
    );
    assert!(matches!(
        told.take()[..],
        [StateMessage::Link {
            link: LinkState::Connecting
        }]
    ));
}

#[tokio::test(start_paused = true)]
async fn an_older_daemon_is_told_only_what_it_knows() {
    let (_, _, mut frames) = connected(22);
    assert_eq!(sent(&mut frames), [ClientFrame::Subscribe]);
}

#[tokio::test(start_paused = true)]
async fn a_snapshot_says_the_link_is_up_then_everything_is_sent_reset() {
    let (mut actor, told, _frames) = connected(44);
    told.take();
    let (snapshot, one) = one_session();
    actor.on_frame(ServerFrame::Snapshot(snapshot));
    assert!(matches!(
        &told.take()[..],
        [StateMessage::Link { link: LinkState::Up { version, protocol: 44 } }] if version == "0.4.1"
    ));
    assert!(actor.flush_at.is_some());
    actor.flush();
    let patches = told.patches();
    assert_eq!(patches.len(), 1);
    assert!(patches[0].reset);
    assert_eq!(patches[0].sessions, [one]);
}

#[tokio::test(start_paused = true)]
async fn patches_come_at_most_one_a_frame() {
    let (mut actor, told, _frames) = connected(44);
    let (snapshot, one) = one_session();
    actor.on_frame(ServerFrame::Snapshot(snapshot));
    actor.flush();
    let flushed = Instant::now();
    told.take();

    tokio::time::advance(Duration::from_millis(1)).await;
    let running = |status| {
        ServerFrame::Delta(Delta::SessionStatus {
            session: one.id,
            status,
            unseen: false,
            since: Timestamp::from_millis(5),
        })
    };
    actor.on_frame(running(SessionStatus::Running));
    assert_eq!(
        actor.flush_at,
        Some(flushed + PATCH_EVERY),
        "a frame after the last patch"
    );
    actor.on_frame(running(SessionStatus::Finished));
    assert_eq!(actor.flush_at, Some(flushed + PATCH_EVERY), "the same one");
    actor.flush();
    let patches = told.patches();
    assert_eq!(patches.len(), 1, "both deltas in one patch");
    assert_eq!(patches[0].sessions[0].status, SessionStatus::Finished);

    // Long after the last patch, the next is due at once.
    tokio::time::advance(Duration::from_secs(1)).await;
    actor.on_frame(running(SessionStatus::Running));
    assert_eq!(actor.flush_at, Some(Instant::now()));
}

#[tokio::test(start_paused = true)]
async fn a_window_that_cannot_be_seen_is_sent_nothing_then_all_that_changed_at_once() {
    let (mut actor, told, _frames) = connected(44);
    let (snapshot, one) = one_session();
    actor.on_frame(ServerFrame::Snapshot(snapshot));
    actor.flush();
    told.take();

    actor.on_request(Request::Looking(Looking {
        seen: false,
        looking: false,
        focused: None,
    }));
    actor.flush();
    for status in [SessionStatus::Running, SessionStatus::Finished] {
        actor.on_frame(ServerFrame::Delta(Delta::SessionStatus {
            session: one.id,
            status,
            unseen: true,
            since: Timestamp::from_millis(5),
        }));
        assert_eq!(actor.flush_at, None, "nothing is due while unseen");
    }
    assert!(told.take().is_empty());

    actor.on_request(Request::Looking(Looking::default()));
    actor.flush();
    let patches = told.patches();
    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].sessions[0].status, SessionStatus::Finished);
    assert_eq!(patches[0].derived.needs_you, [one.id]);
}

#[tokio::test(start_paused = true)]
async fn looking_is_told_as_it_changes_and_again_on_a_new_connection() {
    let (mut actor, _told, mut frames) = connected(44);
    sent(&mut frames);
    let focused = Some(SessionId::new());
    actor.on_request(Request::Looking(Looking {
        seen: true,
        looking: true,
        focused,
    }));
    assert_eq!(sent(&mut frames), [ClientFrame::Keys { session: focused }]);
    actor.on_request(Request::Looking(Looking {
        seen: true,
        looking: false,
        focused: None,
    }));
    assert_eq!(
        sent(&mut frames),
        [
            ClientFrame::Looking { looking: false },
            ClientFrame::Keys { session: None }
        ]
    );
    actor.on_request(Request::Looking(Looking {
        seen: true,
        looking: false,
        focused: None,
    }));
    assert_eq!(sent(&mut frames), [], "nothing changed");
}

#[tokio::test(start_paused = true)]
async fn commands_pass_through_and_are_answered_by_their_own_reply() {
    let (mut actor, _told, mut frames) = connected(44);
    sent(&mut frames);
    let (first, mut first_answer) = oneshot::channel();
    let (second, mut second_answer) = oneshot::channel();
    actor.on_request(Request::Command {
        command: Box::new(Command::Ping),
        reply: first,
    });
    actor.on_request(Request::Command {
        command: Box::new(Command::Ping),
        reply: second,
    });
    assert_eq!(
        sent(&mut frames),
        [
            ClientFrame::Command {
                req_id: 1,
                command: Command::Ping
            },
            ClientFrame::Command {
                req_id: 2,
                command: Command::Ping
            },
        ]
    );
    actor.on_frame(ServerFrame::Reply {
        req_id: 2,
        outcome: Err(ErrorBody::new(ErrorCode::NotFound, "no such project")),
    });
    actor.on_frame(ServerFrame::Reply {
        req_id: 1,
        outcome: Ok(Reply::Done),
    });
    assert!(matches!(first_answer.try_recv(), Ok(Ok(Reply::Done))));
    assert!(matches!(
        second_answer.try_recv(),
        Ok(Err(BridgeError::Refused { code: ErrorCode::NotFound, message })) if message == "no such project"
    ));
}

#[tokio::test(start_paused = true)]
async fn a_lost_link_fails_what_waits_and_says_why_and_since_when_once() {
    let (mut actor, told, _frames) = connected(44);
    told.take();
    let (reply, mut answer) = oneshot::channel();
    actor.on_request(Request::Command {
        command: Box::new(Command::Ping),
        reply,
    });
    actor.on_event(LinkEvent::Lost(
        "the daemon closed the connection".to_owned(),
    ));
    assert!(matches!(
        answer.try_recv(),
        Ok(Err(BridgeError::Unreachable))
    ));
    assert!(!actor.connected.load(Ordering::Relaxed));
    let since = match &told.take()[..] {
        [
            StateMessage::Link {
                link: LinkState::Down { reason, since },
            },
        ] => {
            assert_eq!(reason, "the daemon closed the connection");
            *since
        }
        other => panic!("{other:?}"),
    };
    // Trying again and failing the same way says nothing new.
    actor.on_event(LinkEvent::Lost(
        "the daemon closed the connection".to_owned(),
    ));
    assert!(told.take().is_empty());
    actor.on_event(LinkEvent::Lost("could not reach the daemon".to_owned()));
    assert!(matches!(
        &told.take()[..],
        [StateMessage::Link { link: LinkState::Down { since: still, .. } }] if *still == since
    ));
    // While out of reach a command is refused at once.
    let (reply, mut answer) = oneshot::channel();
    actor.on_request(Request::Command {
        command: Box::new(Command::Ping),
        reply,
    });
    assert!(matches!(
        answer.try_recv(),
        Ok(Err(BridgeError::Unreachable))
    ));
}

#[tokio::test(start_paused = true)]
async fn a_reveal_becomes_a_reveal_message() {
    let (mut actor, told, _frames) = connected(44);
    told.take();
    let session = SessionId::new();
    actor.on_frame(ServerFrame::Reveal { session });
    assert!(matches!(
        &told.take()[..],
        [StateMessage::Reveal { session: revealed }] if *revealed == session
    ));
}

#[tokio::test(start_paused = true)]
async fn a_session_asking_is_read_for_its_question_while_the_window_is_seen() {
    let (mut actor, told, mut frames) = connected(44);
    let (mut snapshot, one) = one_session();
    snapshot.sessions[0].status = SessionStatus::NeedsYou {
        reason: WaitReason::Question,
    };
    actor.on_frame(ServerFrame::Snapshot(snapshot));
    assert!(sent(&mut frames).contains(&ClientFrame::Attach {
        session: one.id,
        from_seq: None,
        watching: false
    }));
    actor.flush();
    told.take();

    actor.on_frame(ServerFrame::Replay {
        session: one.id,
        seq: 0,
        data: b"Which cache?\r\n> shared\r\n".to_vec(),
        reset: true,
        live_from: None,
    });
    tokio::time::advance(PATCH_EVERY).await;
    actor.flush();
    let patches = told.patches();
    assert_eq!(
        patches[0].derived.questions[&one.id],
        ["Which cache?", "> shared"]
    );

    // Hidden, the question is let go; seen again, read again.
    actor.on_request(Request::Looking(Looking {
        seen: false,
        looking: false,
        focused: None,
    }));
    assert!(sent(&mut frames).contains(&ClientFrame::Detach { session: one.id }));
    actor.on_request(Request::Looking(Looking::default()));
    assert!(sent(&mut frames).contains(&ClientFrame::Attach {
        session: one.id,
        from_seq: None,
        watching: false
    }));

    // It stops asking: let go, and the question leaves.
    actor.on_frame(ServerFrame::Delta(Delta::SessionStatus {
        session: one.id,
        status: SessionStatus::Running,
        unseen: false,
        since: Timestamp::from_millis(9),
    }));
    assert!(sent(&mut frames).contains(&ClientFrame::Detach { session: one.id }));
    tokio::time::advance(PATCH_EVERY).await;
    actor.flush();
    let patches = told.patches();
    assert!(patches.last().unwrap().derived.questions.is_empty());
}

#[tokio::test(start_paused = true)]
async fn a_webview_loaded_again_is_told_how_the_link_stands_and_everything_afresh() {
    let (mut actor, told, _frames) = connected(44);
    let (snapshot, _) = one_session();
    actor.on_frame(ServerFrame::Snapshot(snapshot));
    actor.flush();
    told.take();

    let again = Told::default();
    actor.on_request(Request::Open(Box::new(again.clone())));
    tokio::time::advance(PATCH_EVERY).await;
    actor.flush();
    let messages = again.take();
    assert!(matches!(
        messages[0],
        StateMessage::Link {
            link: LinkState::Up { .. }
        }
    ));
    assert!(matches!(&messages[1], StateMessage::Patch { patch } if patch.reset));
}

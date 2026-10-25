//! End-to-end tests of what the notch asks of the daemon (`ARC-22`): who
//! looks at Muster's windows, a session picked in the notch shown in the
//! window looked at last, and Muster's own settings turning the notch on
//! and off, written in the person's own file.

// Helpers outside `#[test]` functions are not covered by clippy.toml's test exemption.
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "end-to-end tests: a failed step is the test failing"
)]
#![expect(
    clippy::disallowed_methods,
    reason = "tests time themselves and start their own threads and tasks; ARC-03, ARC-13 and ARC-14 bind Muster's own code"
)]

mod support;

use std::time::Duration;

use muster_client::{ClientError, Connection, FrameReceiver, FrameSender};
use muster_core::protocol::{ClientFrame, Command, Delta, ErrorCode, Reply, ServerFrame, TermSize};
use muster_core::settings::PreferenceChange;
use muster_core::{SessionId, WorkspaceKind};

use support::{Sandbox, expect_event};

/// A UI's connection, split as the UI holds it.
struct Window {
    sender: FrameSender,
    receiver: FrameReceiver,
}

impl Window {
    fn open(connection: Connection) -> Self {
        let (sender, receiver, _) = connection.into_split();
        Self { sender, receiver }
    }

    async fn looking(&mut self, looking: bool) {
        self.sender
            .send(&ClientFrame::Looking { looking })
            .await
            .unwrap();
    }

    /// The next session the daemon asks this window to show.
    async fn revealed(&mut self) -> SessionId {
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                match self.receiver.next().await.unwrap() {
                    Some(ServerFrame::Reveal { session }) => return session,
                    Some(_) => {}
                    None => panic!("the daemon closed the connection"),
                }
            }
        })
        .await
        .expect("a session to show")
    }
}

async fn looked_at(notch: &mut Connection, wanted: bool) {
    expect_event(notch, |frame| {
        matches!(frame, ServerFrame::Delta(Delta::LookedAt { looked_at }) if *looked_at == wanted)
            .then_some(())
    })
    .await;
}

#[tokio::test]
async fn a_session_picked_in_the_notch_is_shown_in_the_window_looked_at_last() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    let daemon = sandbox.start();
    let mut notch = daemon.connect().await;
    let Reply::ProjectAdded { .. } = notch
        .command(Command::AddProject {
            root,
            name: None,
            space: None,
        })
        .await
        .unwrap()
    else {
        panic!("expected ProjectAdded");
    };
    let snapshot = notch.subscribe().await.unwrap();
    assert!(!snapshot.looked_at);
    let checkout = snapshot
        .workspaces
        .iter()
        .find(|workspace| matches!(workspace.kind, WorkspaceKind::Checkout { .. }))
        .unwrap()
        .id;
    let Reply::SessionStarted { id: session } = notch
        .command(Command::StartSession {
            workspace: Some(checkout),
            harness: "shell".to_owned(),
            title: None,
            size: TermSize::default(),
            prompt: None,
            started_by: None,
            space: None,
        })
        .await
        .unwrap()
    else {
        panic!("expected SessionStarted");
    };

    // No window to show it in.
    let refused = notch.command(Command::Reveal { session }).await;
    assert!(
        matches!(&refused, Err(ClientError::Daemon(error)) if error.code == ErrorCode::NotFound),
        "{refused:?}"
    );

    // Two windows; the person looks at one, then the other, then away.
    // Each window's frames reach the daemon on a connection of their own,
    // so each step waits for the daemon to have heard it.
    let mut first = Window::open(daemon.connect().await);
    let mut second = Window::open(daemon.connect().await);
    first.looking(true).await;
    looked_at(&mut notch, true).await;
    first.looking(false).await;
    looked_at(&mut notch, false).await;
    second.looking(true).await;
    looked_at(&mut notch, true).await;
    second.looking(false).await;
    looked_at(&mut notch, false).await;

    notch.command(Command::Reveal { session }).await.unwrap();
    assert_eq!(second.revealed().await, session, "looked at last");

    // The window looked at last goes: the other one shows it.
    drop(second);
    first.looking(true).await;
    looked_at(&mut notch, true).await;
    notch.command(Command::Reveal { session }).await.unwrap();
    assert_eq!(first.revealed().await, session);

    // Its client gone, however it ended, nobody looks.
    drop(first);
    looked_at(&mut notch, false).await;
}

#[tokio::test]
async fn the_notch_is_turned_off_in_the_person_s_own_settings() {
    let sandbox = Sandbox::new();
    let daemon = sandbox.start();
    let mut notch = daemon.connect().await;
    let snapshot = notch.subscribe().await.unwrap();
    let file = sandbox.paths().config_dir.join("settings.toml");
    assert!(snapshot.preferences.notch);
    assert_eq!(snapshot.preferences.file, file);

    let mut settings = daemon.connect().await;
    assert_eq!(
        settings
            .command(Command::ChangePreference(PreferenceChange::Notch(false)))
            .await
            .unwrap(),
        Reply::Done
    );
    let turned = expect_event(&mut notch, |frame| match frame {
        ServerFrame::Delta(Delta::Preferences(preferences)) => Some(preferences.notch),
        _ => None,
    })
    .await;
    assert!(!turned);
    let written = std::fs::read_to_string(&file).unwrap();
    assert!(written.contains("[notch]\nshow = false"), "{written}");

    // Kept across a restart.
    drop(daemon);
    let daemon = sandbox.start();
    assert!(
        !daemon
            .connect()
            .await
            .subscribe()
            .await
            .unwrap()
            .preferences
            .notch
    );
}

#[tokio::test]
async fn while_muster_s_helper_can_notify_every_window_is_told_and_no_longer_once_it_goes() {
    let sandbox = Sandbox::new();
    let daemon = sandbox.start();
    let mut window = daemon.connect().await;
    let snapshot = window.subscribe().await.unwrap();
    assert!(!snapshot.notifier);
    assert!(snapshot.preferences.notifications, "on unless turned off");
    let mut helper = Window::open(daemon.connect().await);
    helper
        .sender
        .send(&ClientFrame::Notifier { ready: true })
        .await
        .unwrap();
    let ready = |wanted: bool| {
        move |frame: &ServerFrame| match frame {
            ServerFrame::Delta(Delta::Notifier { ready }) if *ready == wanted => Some(()),
            _ => None,
        }
    };
    expect_event(&mut window, ready(true)).await;
    assert!(window.subscribe().await.unwrap().notifier);
    // The helper gone, the windows' terminals tell the person again.
    drop(helper);
    expect_event(&mut window, ready(false)).await;
    assert_eq!(
        window
            .command(Command::ChangePreference(PreferenceChange::Notifications(
                false
            )))
            .await
            .unwrap(),
        Reply::Done
    );
    let written =
        std::fs::read_to_string(sandbox.paths().config_dir.join("settings.toml")).unwrap();
    assert!(
        written.contains("[notifications]\nshow = false"),
        "{written}"
    );

    // Which kinds, and their sounds: kept, and every client told.
    let tell = muster_core::telling::Tell {
        on: true,
        sound: muster_core::telling::Sound::Named("Glass".to_owned()),
    };
    assert_eq!(
        window
            .command(Command::ChangePreference(PreferenceChange::Tell {
                kind: muster_core::telling::Kind::Question,
                tell: tell.clone(),
            }))
            .await
            .unwrap(),
        Reply::Done
    );
    let told = window.subscribe().await.unwrap().preferences.telling;
    assert_eq!(told.questions, tell);
    let written =
        std::fs::read_to_string(sandbox.paths().config_dir.join("settings.toml")).unwrap();
    assert!(
        written.contains("questions = true\nquestions_sound = \"Glass\""),
        "{written}"
    );
}

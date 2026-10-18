//! Kep: the welcome as the UI starts, Kep beside Muster's name where
//! there is nothing to show yet, and its face at the top bar's start,
//! which says how the sessions stand.

use std::path::PathBuf;

use crossterm::event::{KeyCode, MouseButton, MouseEventKind};
use muster_core::protocol::{Delta, ResourceReport, ServerFrame, Snapshot, TermSize};
use muster_core::{SessionId, SessionStatus, Timestamp, WaitReason};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use super::pointer_tests::mouse;
use super::tests::{SIZE, World, press, render, world};
use super::welcome::WELCOME_STEPS;
use super::{App, Dialog, Mode};
use crate::view::look::WORDMARK;

/// The window drawn at `cols` by `rows`, line by line.
fn drawn(app: &App, cols: u16, rows: u16) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(cols, rows)).unwrap();
    terminal
        .draw(|frame| crate::view::draw(frame, app))
        .unwrap();
    let buffer = terminal.backend().buffer();
    (0..rows)
        .map(|y| (0..cols).map(|x| buffer[(x, y)].symbol()).collect())
        .collect()
}

fn top(app: &App) -> String {
    render(app).lines().next().unwrap().to_owned()
}

/// Kep's face, as the top bar starts with it.
fn face(app: &App) -> String {
    top(app).chars().skip(1).take(6).collect()
}

/// Kep large is drawn in half blocks, which nothing else on screen is.
fn kep_large(screen: &str) -> bool {
    screen.contains('▀') || screen.contains('▄')
}

fn starting() -> App {
    let mut app = App::new(PathBuf::from("/src"), SIZE);
    app.welcome = true;
    app
}

fn status(world: &mut World, session: SessionId, status: SessionStatus, unseen: bool) {
    world.app.on_frame(ServerFrame::Delta(Delta::SessionStatus {
        session,
        status,
        unseen,
        since: Timestamp::from_millis(5),
    }));
}

#[test]
fn the_welcome_greets_for_a_second_and_until_the_state_has_come() {
    let mut app = starting();
    let screen = render(&app);
    assert!(kep_large(&screen), "{screen}");
    assert!(screen.contains(WORDMARK[0]), "{screen}");
    assert!(screen.contains("Rounding up your sessions…"), "{screen}");
    assert!(
        !screen.contains("Needs you"),
        "it fills the window: {screen}"
    );

    app.on_connected("0.1.0".to_owned());
    // A second on, the state is not here yet: the welcome waits for it.
    app.on_step(WELCOME_STEPS + 5);
    assert!(app.welcome);
    let world = world();
    app.on_frame(ServerFrame::Snapshot(Snapshot {
        projects: vec![world.project.clone()],
        workspaces: vec![world.worktree.clone()],
        sessions: vec![world.session.clone()],
        ..Snapshot::default()
    }));
    assert!(render(&app).contains("Welcome back. All quiet."));
    assert!(app.on_step(WELCOME_STEPS + 6), "drawn without it");
    assert!(!app.welcome);
    assert!(render(&app).contains("Needs you"));

    // The state come at once, the welcome stays its second.
    let mut app = starting();
    app.on_connected("0.1.0".to_owned());
    app.on_frame(ServerFrame::Snapshot(Snapshot::default()));
    app.on_step(WELCOME_STEPS - 1);
    assert!(app.welcome);
    assert!(render(&app).contains("Agents on your tickets, side by side."));
    app.on_step(WELCOME_STEPS);
    assert!(!app.welcome);
}

#[test]
fn the_welcome_counts_the_sessions_as_the_top_bar_does() {
    let mut world = world();
    world.app.welcome = true;
    let id = world.session.id;
    status(&mut world, id, SessionStatus::Running, false);
    assert!(render(&world.app).contains("Welcome back. 1 working."));
    status(
        &mut world,
        id,
        SessionStatus::NeedsYou {
            reason: WaitReason::Question,
        },
        true,
    );
    assert!(render(&world.app).contains("Welcome back. 1 waiting on you."));
}

#[test]
fn a_daemon_out_of_reach_lets_the_welcome_go_after_its_second() {
    let mut app = starting();
    app.on_disconnected("no daemon at /tmp/muster.sock".to_owned());
    let screen = render(&app);
    assert!(
        screen.contains("The daemon can't be reached; retrying…"),
        "{screen}"
    );
    app.on_step(WELCOME_STEPS);
    assert!(!app.welcome);
    // The screens behind say why.
    assert!(render(&app).contains("no daemon at /tmp/muster.sock"));
}

#[test]
fn a_key_typed_ahead_goes_through_but_a_click_only_lets_the_welcome_go() {
    let mut world = world();
    world.app.welcome = true;
    // A click points at nothing the person can see.
    let click = mouse(MouseEventKind::Down(MouseButton::Left), 50, 10);
    assert!(world.app.on_mouse(click).is_empty());
    assert!(!world.app.welcome);
    assert_eq!(world.app.mode, Mode::Browse, "the pane was not opened");

    world.app.welcome = true;
    let selected = world.app.selected;
    world.app.on_mouse(mouse(MouseEventKind::ScrollDown, 5, 5));
    assert!(!world.app.welcome);
    assert_eq!(world.app.selected, selected, "nor the list moved");
    world.app.on_mouse(mouse(MouseEventKind::ScrollDown, 5, 5));
    assert_ne!(world.app.selected, selected, "as the wheel moves it after");

    // `a` opens the dialog to add a project, as it says.
    world.app.welcome = true;
    press(&mut world.app, KeyCode::Char('a'));
    assert!(!world.app.welcome);
    assert!(
        matches!(world.app.mode, Mode::Dialog(Dialog::AddProject { .. })),
        "{:?}",
        world.app.mode
    );
}

#[test]
fn a_small_window_welcomes_with_the_name_alone() {
    let app = starting();
    let screen = drawn(&app, 40, 12).join("\n");
    assert!(!kep_large(&screen), "{screen}");
    assert!(screen.contains(WORDMARK[0]), "{screen}");
    assert!(screen.contains("Rounding up your"), "{screen}");
    // And a tiny one draws what it can without a panic.
    drawn(&app, 3, 2);
}

#[test]
fn with_nothing_to_show_kep_sits_beside_the_name_in_the_pane() {
    let mut app = App::new(PathBuf::from("/src"), SIZE);
    app.on_connected("0.1.0".to_owned());
    app.on_frame(ServerFrame::Snapshot(Snapshot::default()));
    let screen = render(&app);
    assert!(kep_large(&screen), "{screen}");
    let lines: Vec<&str> = screen.lines().collect();
    let name = lines.iter().position(|line| line.contains(WORDMARK[0]));
    let under = lines
        .iter()
        .position(|line| line.contains("Agents on your tickets, side by side."));
    let press_a = lines.iter().position(|line| {
        line.contains("Press a to add a project: a git repository, or any folder.")
    });
    assert!(
        name < under && under < press_a,
        "the name, the line and the hint in order: {screen}"
    );
    // Too narrow a pane for Kep: the name alone, as before. (Side by
    // side at any width: otherwise a window this narrow shows the list
    // alone.)
    app.on_frame(ServerFrame::Delta(Delta::Preferences(
        muster_core::settings::Preferences {
            side_by_side_from: 0,
            ..muster_core::settings::Preferences::default()
        },
    )));
    app.on_resize(TermSize { cols: 70, rows: 29 });
    let screen = drawn(&app, 70, 30).join("\n");
    assert!(!kep_large(&screen), "{screen}");
    assert!(screen.contains(WORDMARK[2]), "{screen}");
}

#[test]
fn kep_s_face_says_how_the_sessions_stand() {
    let mut world = world();
    let id = world.session.id;
    assert_eq!(face(&world.app), "▼-ᴥ-▼ᶻ", "nothing works: asleep");

    status(&mut world, id, SessionStatus::Running, false);
    assert_eq!(face(&world.app), "▼•ᴥ•▼ ");
    world.app.on_step(38);
    assert_eq!(face(&world.app), "▼-ᴥ-▼ ", "a blink");
    world.app.on_step(40);
    assert_eq!(face(&world.app), "▼•ᴥ•▼ ");

    status(
        &mut world,
        id,
        SessionStatus::NeedsYou {
            reason: WaitReason::Permission,
        },
        false,
    );
    assert_eq!(face(&world.app), "▲•ᴥ•▲!", "ears up, and a bark");

    // A finish gladdens Kep for three seconds; unseen, it then wants a
    // look.
    status(&mut world, id, SessionStatus::Running, false);
    status(&mut world, id, SessionStatus::Finished, true);
    assert_eq!(face(&world.app), "▼^ᴥ^▼ ");
    assert!(world.app.animating());
    world.app.on_step(69);
    assert_eq!(face(&world.app), "▼^ᴥ^▼ ");
    assert!(world.app.on_step(70), "drawn as it ends");
    assert_eq!(face(&world.app), "▲•ᴥ•▲!");
    status(&mut world, id, SessionStatus::Finished, false);
    assert_eq!(face(&world.app), "▼-ᴥ-▼ᶻ");

    world.app.on_disconnected("gone".to_owned());
    assert_eq!(face(&world.app), "▼;ᴥ;▼ ");
}

#[test]
fn with_motion_off_kep_neither_blinks_nor_cheers() {
    let mut world = world();
    world.app.motion = false;
    let id = world.session.id;
    status(&mut world, id, SessionStatus::Running, false);
    world.app.on_step(38);
    assert_eq!(face(&world.app), "▼•ᴥ•▼ ");
    status(&mut world, id, SessionStatus::Finished, false);
    assert_eq!(face(&world.app), "▼-ᴥ-▼ᶻ");
    assert!(!world.app.animating());
}

#[test]
fn kep_is_the_first_to_go_short_of_room() {
    let mut world = world();
    let id = world.session.id;
    status(&mut world, id, SessionStatus::Running, false);
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::Resources(ResourceReport {
            host_memory_used: 4_000_000_000,
            host_memory_total: 16_000_000_000,
            sessions: Vec::new(),
        })));
    let (mut with_kep, mut all_but_kep) = (false, false);
    for cols in 60..=160 {
        let bar = drawn(&world.app, cols, SIZE.rows + 1).remove(0);
        let kep = bar.starts_with(" ▼•ᴥ•▼");
        let all = bar.contains("1 working")
            && bar.contains("4.0/16.0 GB")
            && bar.contains("daemon 0.1.0");
        // Kep only beside all the rest.
        assert!(!kep || all, "{bar}");
        with_kep |= kep;
        all_but_kep |= all && !kep;
    }
    assert!(with_kep && all_but_kep);
}

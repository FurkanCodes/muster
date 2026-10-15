//! Motion: a running session's mark spins, what needs the person pulses,
//! a line flashes as its session comes to want a look, and nothing moves
//! otherwise.

use muster_core::protocol::{Delta, ServerFrame};
use muster_core::{SessionStatus, WaitReason};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::{Color, Modifier};

use super::tests::{SIZE, World, render, world};

fn with_status(world: &mut World, status: SessionStatus) {
    let mut session = world.session.clone();
    session.status = status;
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::SessionUpserted(session)));
}

fn buffer(world: &World) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(SIZE.cols, SIZE.rows + 1)).unwrap();
    terminal
        .draw(|frame| crate::view::draw(frame, &world.app))
        .unwrap();
    terminal.backend().buffer().clone()
}

/// The modifiers of the first cell showing `symbol`.
fn modifiers_of(buffer: &Buffer, symbol: &str) -> Modifier {
    // Below the top bar, where Kep's ears are.
    buffer
        .content
        .iter()
        .skip(usize::from(buffer.area.width))
        .find(|cell| cell.symbol() == symbol)
        .unwrap_or_else(|| panic!("no {symbol} on screen"))
        .modifier
}

/// The modifiers of the Needs you tab's count.
fn count_modifiers(buffer: &Buffer) -> Modifier {
    let top: String = (0..buffer.area.width)
        .map(|x| buffer[(x, 0)].symbol())
        .collect();
    let at = top
        .chars()
        .collect::<Vec<_>>()
        .windows(10)
        .position(|window| window.iter().collect::<String>() == "Needs you ")
        .expect("the Needs you tab");
    buffer[(u16::try_from(at + 10).unwrap(), 0)].modifier
}

#[test]
fn a_running_session_s_mark_spins_one_frame_a_step() {
    let mut world = world();
    with_status(&mut world, SessionStatus::Running);
    world.app.step = 0;
    let screen = render(&world.app);
    assert!(screen.contains("⠋ Shell  running"), "{screen}");
    assert!(!screen.contains('●'), "{screen}");
    world.app.step = 1;
    assert!(render(&world.app).contains("⠙ Shell  running"));
    world.app.step = 10;
    assert!(render(&world.app).contains("⠋ Shell  running"));
}

#[test]
fn what_needs_the_person_pulses_bright_plain_dim_plain() {
    let mut world = world();
    with_status(
        &mut world,
        SessionStatus::NeedsYou {
            reason: WaitReason::Permission,
        },
    );
    let at = |world: &mut World, step| {
        world.app.step = step;
        let buffer = buffer(world);
        (modifiers_of(&buffer, "▲"), count_modifiers(&buffer))
    };
    // From step 16, once the line's flash for the change has settled.
    let (mark, count) = at(&mut world, 16);
    assert!(mark.contains(Modifier::BOLD) && !mark.contains(Modifier::DIM));
    assert!(count.contains(Modifier::BOLD) && !count.contains(Modifier::DIM));
    let (mark, count) = at(&mut world, 20);
    assert!(!mark.intersects(Modifier::BOLD | Modifier::DIM));
    assert!(!count.intersects(Modifier::BOLD | Modifier::DIM));
    let (mark, count) = at(&mut world, 24);
    assert!(mark.contains(Modifier::DIM) && !mark.contains(Modifier::BOLD));
    assert!(count.contains(Modifier::DIM) && !count.contains(Modifier::BOLD));
}

#[test]
fn with_motion_off_every_mark_stands_still() {
    let mut world = world();
    world.app.motion = false;
    world.app.step = 7;
    with_status(&mut world, SessionStatus::Running);
    assert!(render(&world.app).contains("● Shell  running"));
    assert!(!world.app.animating());

    with_status(
        &mut world,
        SessionStatus::NeedsYou {
            reason: WaitReason::Question,
        },
    );
    world.app.step = 8;
    let buffer = buffer(&world);
    assert!(modifiers_of(&buffer, "▲").contains(Modifier::BOLD));
    assert!(count_modifiers(&buffer).contains(Modifier::BOLD));
    assert!(!world.app.animating());
}

/// Lets any flash for the last change settle, and Kep's gladness of a
/// finish.
fn settle(world: &mut World) {
    world
        .app
        .on_step(world.app.step + crate::motion::GLAD_STEPS);
    world.app.on_step(world.app.step + 1);
}

#[test]
fn only_a_running_session_or_something_waiting_asks_for_drawing() {
    let mut world = world();
    assert!(!world.app.animating(), "an idle session stands still");
    with_status(&mut world, SessionStatus::Finished);
    assert!(world.app.animating(), "its line flashes");
    settle(&mut world);
    assert!(!world.app.animating(), "then a finished one stands still");
    with_status(&mut world, SessionStatus::Exited { success: false });
    settle(&mut world);
    assert!(!world.app.animating(), "and an ended one");

    with_status(&mut world, SessionStatus::Running);
    assert!(world.app.animating());
    with_status(
        &mut world,
        SessionStatus::NeedsYou {
            reason: WaitReason::Startup,
        },
    );
    assert!(world.app.animating());

    // News not yet seen waits on the person too: its count pulses.
    let mut news = world.session.clone();
    news.status = SessionStatus::Finished;
    news.unseen = true;
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::SessionUpserted(news)));
    assert!(world.app.animating());
}

#[test]
fn an_idle_screen_wants_no_steps_and_its_times_change_by_the_minute() {
    let mut world = world();
    settle(&mut world);
    // An idle session: nothing moves, so the UI takes no steps.
    assert!(!world.app.wants_steps());
    assert_eq!(world.app.clock_every(), Some(60_000));
    // A note is let go after a while: steps until it has gone.
    world.app.note = Some(super::Note {
        text: "Copied".to_owned(),
        error: false,
        shown_at: world.app.step,
    });
    assert!(world.app.wants_steps());
    world.app.on_step(world.app.step + crate::motion::NOTE_GOES);
    assert!(!world.app.wants_steps());
    // A running session spins, and its time reads by the second.
    with_status(&mut world, SessionStatus::Running);
    assert!(world.app.wants_steps());
    assert_eq!(world.app.clock_every(), Some(1_000));
}

/// The cell where `text` first starts on screen.
fn cell_at<'a>(buffer: &'a Buffer, text: &str) -> &'a ratatui::buffer::Cell {
    for y in 0..buffer.area.height {
        let row: Vec<&str> = (0..buffer.area.width)
            .map(|x| buffer[(x, y)].symbol())
            .collect();
        let line: String = row.concat();
        if let Some(at) = line.find(text) {
            let x = line[..at].chars().count();
            return &buffer[(u16::try_from(x).unwrap(), y)];
        }
    }
    panic!("no {text:?} on screen");
}

#[test]
fn a_session_that_finishes_flashes_lit_then_tinted_then_settles() {
    let mut world = world();
    world.app.on_step(20);
    with_status(&mut world, SessionStatus::Running);
    assert_eq!(
        world.app.flash_age(world.session.id),
        None,
        "running: no flash"
    );

    with_status(&mut world, SessionStatus::Finished);
    assert_eq!(world.app.flash_age(world.session.id), Some(0));
    let lit = buffer(&world);
    let title = cell_at(&lit, "Shell  finished");
    assert_eq!((title.fg, title.bg), (Color::Black, Color::Green));
    // Lit over the spans' own styles: the mark stays readable on it.
    assert_eq!(cell_at(&lit, "✓ Shell").bg, Color::Green);

    assert!(world.app.on_step(25), "a flashing line is drawn each step");
    let tinted = buffer(&world);
    let title = cell_at(&tinted, "Shell  finished");
    assert_eq!((title.fg, title.bg), (Color::Green, Color::Reset));
    assert!(title.modifier.contains(Modifier::BOLD));

    // Once over, one more frame settles the line, then nothing asks.
    assert!(world.app.on_step(30));
    assert_eq!(world.app.flash_age(world.session.id), None);
    let settled = buffer(&world);
    let title = cell_at(&settled, "Shell  finished");
    assert_ne!(title.fg, Color::Green);
    assert_eq!(title.bg, Color::Reset);
    // Kep is glad of it three seconds from the finish; once that is over,
    // one more frame, then nothing asks.
    assert!(world.app.on_step(49));
    assert!(world.app.on_step(50));
    assert!(!world.app.on_step(51));
}

#[test]
fn waiting_and_ending_flash_but_starting_or_staying_the_same_does_not() {
    let mut world = world();
    let id = world.session.id;
    world.app.on_step(3);
    with_status(&mut world, SessionStatus::Running);
    assert_eq!(world.app.flash_age(id), None);

    with_status(
        &mut world,
        SessionStatus::NeedsYou {
            reason: WaitReason::Question,
        },
    );
    assert_eq!(world.app.flash_age(id), Some(0));
    // The same status again does not start the flash over.
    world.app.on_step(6);
    with_status(
        &mut world,
        SessionStatus::NeedsYou {
            reason: WaitReason::Question,
        },
    );
    assert_eq!(world.app.flash_age(id), Some(3));

    world.app.on_step(40);
    with_status(&mut world, SessionStatus::Exited { success: false });
    assert_eq!(world.app.flash_age(id), Some(0));

    // A new session, or a snapshot, flashes nothing.
    world.app.on_step(80);
    let mut new = world.session.clone();
    new.id = muster_core::SessionId::new();
    new.status = SessionStatus::Finished;
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::SessionUpserted(new.clone())));
    assert_eq!(world.app.flash_age(new.id), None);
    let mut finished = world.session.clone();
    finished.status = SessionStatus::Finished;
    world
        .app
        .on_frame(ServerFrame::Snapshot(muster_core::protocol::Snapshot {
            projects: vec![world.project.clone()],
            workspaces: vec![world.worktree.clone()],
            sessions: vec![finished],
            ..muster_core::protocol::Snapshot::default()
        }));
    assert_eq!(world.app.flash_age(id), None);
}

#[test]
fn with_motion_off_nothing_flashes() {
    let mut world = world();
    world.app.motion = false;
    world.app.on_step(1);
    with_status(&mut world, SessionStatus::Running);
    with_status(&mut world, SessionStatus::Finished);
    assert_eq!(world.app.flash_age(world.session.id), None);
    assert_eq!(cell_at(&buffer(&world), "Shell  finished").bg, Color::Reset);
    assert!(!world.app.on_step(2));
}

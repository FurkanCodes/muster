//! Motion: a running session's mark spins, what needs the person pulses,
//! and nothing moves otherwise.

use muster_core::protocol::{Delta, ServerFrame};
use muster_core::{SessionStatus, WaitReason};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::Modifier;

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
    buffer
        .content
        .iter()
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
    let (mark, count) = at(&mut world, 0);
    assert!(mark.contains(Modifier::BOLD) && !mark.contains(Modifier::DIM));
    assert!(count.contains(Modifier::BOLD) && !count.contains(Modifier::DIM));
    let (mark, count) = at(&mut world, 4);
    assert!(!mark.intersects(Modifier::BOLD | Modifier::DIM));
    assert!(!count.intersects(Modifier::BOLD | Modifier::DIM));
    let (mark, count) = at(&mut world, 8);
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

#[test]
fn only_a_running_session_or_something_waiting_asks_for_drawing() {
    let mut world = world();
    assert!(!world.app.animating(), "an idle session stands still");
    with_status(&mut world, SessionStatus::Finished);
    assert!(!world.app.animating(), "a finished one too");
    with_status(&mut world, SessionStatus::Exited { success: false });
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

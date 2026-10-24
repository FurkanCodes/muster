//! What a chat is called (`PRODUCT.md` §6): its agent's mark before its
//! name, drawn as a picture where the terminal can, and `R` renaming it.

use crossterm::event::KeyCode;
use muster_core::Session;
use muster_core::protocol::{Command, Delta, ServerFrame};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::{Buffer, CellDiffOption};

use super::super::tests::{SIZE, World, command, press, render, world};
use super::super::{Dialog, Mode, View};
use crate::model::Row;
use crate::view::logos::Graphics;

fn with_harness(world: &mut World, harness: &str) -> Session {
    let session = Session {
        harness: harness.to_owned(),
        title: "Fix login".to_owned(),
        ..world.session.clone()
    };
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::SessionUpserted(session.clone())));
    session
}

/// The frame drawn, as the app left its cells.
fn buffer(world: &World) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(SIZE.cols, SIZE.rows + 1)).unwrap();
    terminal
        .draw(|frame| crate::view::draw(frame, &world.app))
        .unwrap()
        .buffer
        .clone()
}

#[test]
fn each_chat_is_led_by_its_agent_s_mark_and_named() {
    let mut world = world();
    with_harness(&mut world, "claude");
    let screen = render(&world.app);
    assert!(screen.contains("○ ✻ Fix login"), "{screen}");
    with_harness(&mut world, "gemini");
    assert!(render(&world.app).contains("○ ✦ Fix login"));
}

#[test]
fn where_the_terminal_draws_pictures_the_mark_is_one() {
    let mut world = world();
    world.app.graphics = Graphics::Iterm;
    with_harness(&mut world, "claude");
    let cells = buffer(&world);
    let (x, y) = (0..cells.area.height)
        .flat_map(|y| (0..cells.area.width).map(move |x| (x, y)))
        .find(|at| cells[*at].symbol().starts_with("\x1b]1337;File="))
        .expect("a picture");
    // After the list's border and edge, the line's indent and its mark.
    assert_eq!(x, 1 + 1 + 3 + 2);
    assert_ne!(cells[(x, y)].diff_option, CellDiffOption::None);
    assert_eq!(cells[(x + 1, y)].diff_option, CellDiffOption::Skip);
    let line: String = (x + 3..x + 12).map(|at| cells[(at, y)].symbol()).collect();
    assert_eq!(line, "Fix login");
}

#[test]
fn r_renames_the_selected_chat_once_the_person_says_what_to() {
    let mut world = world();
    let id = world.session.id;
    world.app.selected = Some(Row::Session(id));
    press(&mut world.app, KeyCode::Char('R'));
    let Mode::Dialog(Dialog::Rename { session, name }) = &world.app.mode else {
        panic!("{:?}", world.app.mode);
    };
    assert_eq!((*session, name.text()), (id, "Shell"));
    assert!(render(&world.app).contains("What to call this chat:"));

    // The name as it is, to change: cleared and typed anew.
    for _ in 0.."Shell".len() {
        press(&mut world.app, KeyCode::Backspace);
    }
    // Nothing to call it: the field stays.
    assert!(press(&mut world.app, KeyCode::Enter).is_empty());
    assert!(matches!(
        world.app.mode,
        Mode::Dialog(Dialog::Rename { .. })
    ));
    for c in "Login fix".chars() {
        press(&mut world.app, KeyCode::Char(c));
    }
    let (_, asked) = command(&press(&mut world.app, KeyCode::Enter));
    assert_eq!(
        asked,
        Command::RenameSession {
            session: id,
            title: "Login fix".to_owned()
        }
    );
    assert_eq!(world.app.mode, Mode::Browse);

    // Esc keeps the name.
    press(&mut world.app, KeyCode::Char('R'));
    let sent = press(&mut world.app, KeyCode::Esc);
    assert!(
        !sent.iter().any(|action| matches!(
            action,
            super::super::Action::Send(muster_core::protocol::ClientFrame::Command { .. })
        )),
        "{sent:?}"
    );
    assert_eq!(world.app.mode, Mode::Browse);
}

#[test]
fn r_needs_a_session() {
    let mut world = world();
    world.app.selected = Some(Row::Worktree(world.worktree.id));
    assert!(press(&mut world.app, KeyCode::Char('R')).is_empty());
    assert!(render(&world.app).contains("Select a session to rename it."));

    world.app.view = View::NeedsYou;
    world.app.needs_selected = Some(world.session.id);
}

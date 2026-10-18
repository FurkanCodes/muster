//! Every key of the screen shown, over it, on `h` (or `?`), narrowed as
//! the person types: so the keys can be found without knowing them.

use crossterm::event::KeyCode;

use super::tests::{press, render, typed, world};
use super::{Mode, View};
use crate::model::Row;

#[test]
fn h_shows_every_key_of_the_screen_shown() {
    let mut world = world();
    let screen = render(&world.app);
    assert!(
        screen.contains("h help"),
        "the bottom bar says so: {screen}"
    );
    press(&mut world.app, KeyCode::Char('h'));
    assert!(matches!(world.app.mode, Mode::Help(_)));
    let screen = render(&world.app);
    assert!(screen.contains("Keys · Sessions"), "{screen}");
    for wanted in ["On the list", "the session full screen", "fold or unfold"] {
        assert!(screen.contains(wanted), "{wanted}: {screen}");
    }
    // The rest a page down.
    assert!(!screen.contains("Shift-Q"), "{screen}");
    press(&mut world.app, KeyCode::PageDown);
    let screen = render(&world.app);
    assert!(screen.contains("In a session"), "{screen}");
    assert!(screen.contains("Shift-Q"), "{screen}");
    press(&mut world.app, KeyCode::Esc);
    assert_eq!(world.app.mode, Mode::Browse);
}

#[test]
fn typing_narrows_the_keys_to_those_that_say_it() {
    let mut world = world();
    press(&mut world.app, KeyCode::Char('h'));
    // Letters go to the filter, not to the list behind.
    typed(&mut world.app, "pull");
    assert!(matches!(world.app.mode, Mode::Help(_)));
    let screen = render(&world.app);
    assert!(screen.contains("Find pull"), "{screen}");
    assert!(screen.contains("pull request in the browser"), "{screen}");
    assert!(!screen.contains("add a project"), "{screen}");
    assert!(!screen.contains("In a session"), "headings with nothing go");
    // By its key too, ignoring case.
    for _ in 0.."pull".len() {
        press(&mut world.app, KeyCode::Backspace);
    }
    typed(&mut world.app, "SHIFT-Q");
    let screen = render(&world.app);
    assert!(screen.contains("back to the list"), "{screen}");
    typed(&mut world.app, "zzz");
    assert!(render(&world.app).contains("No key says"));
    // ⏎ closes it too, and nothing typed reached the list.
    press(&mut world.app, KeyCode::Enter);
    assert_eq!(world.app.mode, Mode::Browse);
    assert_eq!(world.app.selected, Some(Row::Project(world.project.id)));
}

#[test]
fn question_mark_opens_it_too_on_any_tab() {
    let mut world = world();
    press(&mut world.app, KeyCode::Char('3'));
    assert_eq!(world.app.view, View::Tickets);
    press(&mut world.app, KeyCode::Char('?'));
    let screen = render(&world.app);
    assert!(screen.contains("Keys · Tickets"), "{screen}");
    assert!(screen.contains("fetch the tickets again"), "{screen}");
    press(&mut world.app, KeyCode::Esc);
    press(&mut world.app, KeyCode::Char('4'));
    press(&mut world.app, KeyCode::Char('h'));
    assert!(render(&world.app).contains("Keys · Settings"));
}

#[test]
fn over_the_changes_it_lists_theirs_and_goes_back_to_them() {
    let mut world = world();
    world.app.selected = Some(Row::Worktree(world.worktree.id));
    press(&mut world.app, KeyCode::Char('d'));
    assert_eq!(world.app.mode, Mode::Changes);
    // There `h` goes back from a diff, so `?` alone opens it.
    press(&mut world.app, KeyCode::Char('?'));
    let screen = render(&world.app);
    assert!(screen.contains("Keys · Changes"), "{screen}");
    assert!(screen.contains("put the files back"), "{screen}");
    press(&mut world.app, KeyCode::Esc);
    assert_eq!(world.app.mode, Mode::Changes);
}

#[test]
fn the_help_key_is_not_shown_while_a_session_has_the_keys() {
    let mut world = world();
    world.app.selected = Some(Row::Session(world.session.id));
    press(&mut world.app, KeyCode::Enter);
    let screen = render(&world.app);
    assert!(!screen.contains("h help"), "{screen}");
    // And h reaches the program.
    let actions = press(&mut world.app, KeyCode::Char('h'));
    assert_eq!(world.app.mode, Mode::Terminal(world.session.id));
    assert!(!actions.is_empty());
}

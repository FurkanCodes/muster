//! The person's own keys (`PRODUCT.md` §6): set on the Settings screen's
//! *Keys*, refused where another action has the key, and once kept, what
//! the keys do, and what the hints, the help and the menus say.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use muster_core::keys::Action as Does;
use muster_core::protocol::{ClientFrame, Command};
use muster_core::settings::PreferenceChange;

use super::tests::{World, preferences, press, render, world};
use super::{Action, Dialog, Mode, SettingRow, View};
use crate::model::Row;

fn changed(actions: &[Action]) -> Option<PreferenceChange> {
    actions.iter().find_map(|action| match action {
        Action::Send(ClientFrame::Command {
            command: Command::ChangePreference(change),
            ..
        }) => Some(change.clone()),
        _ => None,
    })
}

/// The Keys list open on `does`.
fn on(world: &mut World, does: Does) {
    press(&mut world.app, KeyCode::Char('4'));
    world.app.settings.selected = Some(SettingRow::Keys);
    press(&mut world.app, KeyCode::Enter);
    let Mode::Dialog(Dialog::Keys(rebinding)) = &mut world.app.mode else {
        panic!("{:?}", world.app.mode);
    };
    rebinding.chosen = Does::ALL.iter().position(|each| *each == does).unwrap();
}

fn shifted(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::SHIFT)
}

/// The person's keys, as the daemon would say once it keeps them.
fn kept(world: &mut World, set: &[(&str, &[&str])]) {
    preferences(&mut world.app, |preferences| {
        preferences.keys = set
            .iter()
            .map(|(action, keys)| {
                (
                    (*action).to_owned(),
                    keys.iter().map(|key| (*key).to_owned()).collect(),
                )
            })
            .collect();
    });
}

#[test]
fn a_key_is_set_by_pressing_it_and_does_what_it_was_set_to() {
    let mut world = world();
    on(&mut world, Does::NewSession);
    let screen = render(&world.app);
    assert!(screen.contains("a new session in the worktree"), "{screen}");
    press(&mut world.app, KeyCode::Enter);
    assert!(render(&world.app).contains("Press the key for: a new session"));
    let actions = world.app.on_key(shifted('N'));
    assert_eq!(
        changed(&actions),
        Some(PreferenceChange::Keys {
            action: "new_session".to_owned(),
            keys: Some(vec!["N".to_owned()]),
        })
    );
    kept(&mut world, &[("new_session", &["N"])]);
    assert!(render(&world.app).contains("· yours"));
    press(&mut world.app, KeyCode::Esc);
    press(&mut world.app, KeyCode::Char('1'));
    world.app.selected = Some(Row::Worktree(world.worktree.id));
    press(&mut world.app, KeyCode::Char('n'));
    assert_eq!(world.app.mode, Mode::Browse, "n does nothing now");
    world.app.on_key(shifted('N'));
    assert!(matches!(
        world.app.mode,
        Mode::Dialog(Dialog::NewSession { .. })
    ));
}

#[test]
fn a_key_another_action_has_or_a_session_s_program_would_take_is_refused() {
    let mut world = world();
    on(&mut world, Does::NewSession);
    press(&mut world.app, KeyCode::Enter);
    let actions = press(&mut world.app, KeyCode::Char('d'));
    assert_eq!(changed(&actions), None);
    let note = world.app.note.clone().unwrap();
    assert!(note.error && note.text.contains("show_changes"), "{note:?}");
    assert!(matches!(world.app.mode, Mode::Dialog(Dialog::Keys(_))));

    press(&mut world.app, KeyCode::Esc);
    on(&mut world, Does::LeaveSession);
    press(&mut world.app, KeyCode::Enter);
    let actions = press(&mut world.app, KeyCode::Char('x'));
    assert_eq!(changed(&actions), None);
    let note = world.app.note.clone().unwrap();
    assert!(note.text.contains("needs ctrl or alt"), "{note:?}");
    // Esc while waiting keeps the key as it is, and the list open.
    press(&mut world.app, KeyCode::Enter);
    press(&mut world.app, KeyCode::Esc);
    assert!(matches!(
        &world.app.mode,
        Mode::Dialog(Dialog::Keys(rebinding)) if !rebinding.waiting
    ));
}

#[test]
fn backspace_puts_an_action_s_keys_back_and_shift_r_every_key() {
    let mut world = world();
    kept(&mut world, &[("rename", &["F2"])]);
    on(&mut world, Does::Rename);
    assert_eq!(
        changed(&press(&mut world.app, KeyCode::Backspace)),
        Some(PreferenceChange::Keys {
            action: "rename".to_owned(),
            keys: None,
        })
    );
    assert_eq!(
        changed(&world.app.on_key(shifted('R'))),
        Some(PreferenceChange::AllKeysBack)
    );
    // An action with Muster's keys has nothing to put back.
    press(&mut world.app, KeyCode::Esc);
    on(&mut world, Does::Quit);
    assert_eq!(changed(&press(&mut world.app, KeyCode::Backspace)), None);
}

#[test]
fn the_hints_the_help_and_the_menu_show_the_keys_as_set() {
    let mut world = world();
    kept(&mut world, &[("rename", &["F2"]), ("full_screen", &["y"])]);
    world.app.selected = Some(Row::Session(world.session.id));
    let screen = render(&world.app);
    assert!(screen.contains("y full screen"), "{screen}");
    let items: Vec<String> = world
        .app
        .actions_here()
        .into_iter()
        .map(|here| format!("{} {}", here.key, here.what))
        .collect();
    assert!(items.contains(&"F2 rename".to_owned()), "{items:?}");
    press(&mut world.app, KeyCode::Char('h'));
    let help = render(&world.app);
    assert!(help.contains("F2"), "{help}");
    assert!(!help.contains("Shift-R"), "{help}");
}

#[test]
fn a_session_is_left_on_the_key_the_person_set_and_the_old_one_reaches_its_program() {
    let mut world = world();
    kept(&mut world, &[("leave_session", &["alt-q"])]);
    world.app.selected = Some(Row::Session(world.session.id));
    press(&mut world.app, KeyCode::Enter);
    assert!(matches!(world.app.mode, Mode::Terminal(_)));
    let typed = world.app.on_key(shifted('Q'));
    assert!(
        typed
            .iter()
            .any(|action| matches!(action, Action::Send(ClientFrame::Input { .. }))),
        "{typed:?}"
    );
    world
        .app
        .on_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::ALT));
    assert_eq!(world.app.mode, Mode::Browse);
    assert_eq!(world.app.view, View::Sessions);
}

#[test]
fn a_key_for_an_action_no_longer_there_leaves_the_rest_and_says_so() {
    let mut world = world();
    // A plugin's panel's key, its plugin removed, beside one of Muster's.
    kept(
        &mut world,
        &[("zoetrope_graph", &["Z"]), ("new_session", &["N"])],
    );
    assert_eq!(
        world.app.keymap.label(Does::NewSession),
        "Shift-N",
        "the rest read"
    );
    assert_eq!(world.app.keymap.aside().len(), 1);
    press(&mut world.app, KeyCode::Char('4'));
    world.app.settings.selected = Some(SettingRow::Keys);
    world.app.on_resize(muster_core::protocol::TermSize {
        cols: 180,
        rows: 40,
    });
    let screen = render(&world.app);
    assert!(screen.contains("1 of yours"), "{screen}");
    assert!(
        screen.contains("keys.zoetrope_graph is not something a key does"),
        "{screen}"
    );
}

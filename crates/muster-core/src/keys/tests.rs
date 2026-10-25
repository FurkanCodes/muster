//! Muster's keys read, checked, set and shown.

use super::*;

#[test]
fn every_default_key_reads() {
    for action in Action::ALL {
        for key in action.defaults() {
            assert!(Chord::parse(key).is_ok(), "{action:?}: {key}");
        }
    }
}

fn set(pairs: &[(&str, &[&str])]) -> BTreeMap<String, Vec<String>> {
    pairs
        .iter()
        .map(|(name, keys)| {
            (
                (*name).to_owned(),
                keys.iter().map(|key| (*key).to_owned()).collect(),
            )
        })
        .collect()
}

#[test]
fn keys_read_and_write_as_written() {
    for text in [
        "a",
        "R",
        "?",
        "ctrl-]",
        "alt-x",
        "ctrl-\\",
        "tab",
        "shift-tab",
        "esc",
        "backspace",
        "space",
        "f5",
        "1",
    ] {
        assert_eq!(Chord::parse(text).unwrap().to_string(), text, "{text}");
    }
    assert_eq!(Chord::parse("shift-q").unwrap(), Chord::parse("Q").unwrap());
    assert_eq!(Chord::parse("Ctrl-]").unwrap().label(), "Ctrl-]");
    assert_eq!(Chord::parse("tab").unwrap().label(), "⇥");
    for not in ["", "ctrl-", "abc", "f99", "\u{7}"] {
        assert!(
            matches!(Chord::parse(not), Err(KeysError::NotAKey(_))),
            "{not:?}"
        );
    }
}

#[test]
fn each_action_is_listed_once_under_where_it_works() {
    assert_eq!(Action::Quit.group(), "Anywhere");
    assert_eq!(Action::Back.group(), "Anywhere");
    assert_eq!(Action::FoldList.group(), "Anywhere");
    assert_eq!(Action::NewSession.group(), "Sessions and Needs you");
    assert_eq!(Action::Tidy.group(), "Sessions");
    assert_eq!(Action::CloseTicket.group(), "Tickets");
    assert_eq!(Action::PutBack.group(), "In the changes");
    assert_eq!(Action::LeaveSession.group(), "In a session");
    assert_eq!(Action::PanelKey.group(), "Anywhere");
    assert_eq!(Action::SplitRight.group(), "After the panel key");
    // Each group's actions come together.
    let mut seen: Vec<&str> = Vec::new();
    for action in Action::ALL {
        if seen.last() != Some(&action.group()) {
            assert!(!seen.contains(&action.group()), "{} again", action.group());
            seen.push(action.group());
        }
    }
}

#[test]
fn the_panel_key_is_never_typed_and_the_key_after_it_may_be_a_plain_letter() {
    let keymap = Keymap::default();
    let chord = |text: &str| Chord::parse(text).unwrap();
    assert_eq!(
        keymap.action(Screen::InSession, chord("ctrl-b")),
        Some(Action::PanelKey)
    );
    assert_eq!(
        keymap.action(Screen::Sessions, chord("ctrl-b")),
        Some(Action::PanelKey)
    );
    assert_eq!(
        keymap.action(Screen::Prefix, chord("v")),
        Some(Action::SplitRight)
    );
    assert_eq!(
        keymap.action(Screen::Prefix, chord("7")),
        Some(Action::ToPanel)
    );
    // After the panel key, a letter is Muster's; the panel key itself
    // reaches a session, so it is never one its program takes as typed.
    let error = Keymap::with(&set(&[("panel_key", &["b"])])).unwrap_err();
    assert!(matches!(error, KeysError::Typed { .. }), "{error}");
    assert!(Keymap::with(&set(&[("panel_key", &["ctrl-a"])])).is_ok());
    assert!(
        Keymap::with(&set(&[("zoom", &["m"])])).is_ok(),
        "m is the list's, not here"
    );
    let error = Keymap::with(&set(&[("zoom", &["v"])])).unwrap_err();
    assert!(matches!(error, KeysError::Taken { .. }), "{error}");
}

#[test]
fn muster_s_own_keys_never_clash() {
    assert_eq!(Keymap::with(&BTreeMap::new()), Ok(Keymap::default()));
    let names: std::collections::BTreeSet<&str> =
        Action::ALL.iter().map(|action| action.name()).collect();
    assert_eq!(names.len(), Action::ALL.len(), "each named once");
}

#[test]
fn a_key_does_what_its_screen_says() {
    let keys = Keymap::default();
    let r = Chord::parse("r").unwrap();
    assert_eq!(
        keys.action(Screen::Sessions, r),
        Some(Action::RestartSession)
    );
    assert_eq!(
        keys.action(Screen::Tickets, r),
        Some(Action::RefreshTickets)
    );
    assert_eq!(keys.action(Screen::Changes, r), Some(Action::ChangesAgain));
    assert_eq!(keys.action(Screen::InSession, r), None, "the program's");
    let leave = Chord::parse("Q").unwrap();
    assert_eq!(
        keys.action(Screen::InSession, leave),
        Some(Action::LeaveSession)
    );
    assert_eq!(keys.action(Screen::Sessions, leave), None);
    assert_eq!(keys.label(Action::Rename), "Shift-R");
    assert_eq!(keys.labels(Action::Help), "h ?");
}

#[test]
fn the_person_s_keys_take_the_place_of_muster_s() {
    let keys = Keymap::with(&set(&[("new_session", &["N"]), ("rename", &["F2"])])).unwrap();
    assert_eq!(
        keys.action(Screen::Sessions, Chord::parse("N").unwrap()),
        Some(Action::NewSession)
    );
    assert_eq!(
        keys.action(Screen::Sessions, Chord::parse("n").unwrap()),
        None
    );
    assert_eq!(keys.label(Action::Rename), "F2");
    assert!(!keys.is_default(Action::Rename));
    // None at all: the action goes without a key.
    let keys = Keymap::with(&set(&[("move_panels", &[])])).unwrap();
    assert_eq!(keys.label(Action::MovePanels), "none");
}

#[test]
fn a_key_another_action_has_on_the_same_screen_is_refused() {
    assert_eq!(
        Keymap::with(&set(&[("new_session", &["d"])])),
        Err(KeysError::Taken {
            action: "new_session".to_owned(),
            key: "d".to_owned(),
            other: "show_changes".to_owned(),
        })
    );
    // On screens they never share, two actions may have one key.
    assert!(Keymap::with(&set(&[("refresh_tickets", &["n"])])).is_ok());
}

#[test]
fn a_terminal_tab_is_left_on_a_letter_with_ctrl_or_alt_prd_6b() {
    let keys = Keymap::default();
    assert_eq!(
        keys.action(Screen::InTab, Chord::parse("ctrl-q").unwrap()),
        Some(Action::LeaveTab)
    );
    assert_eq!(Action::LeaveTab.group(), "In a terminal tab");
    assert!(Keymap::with(&set(&[("leave_tab", &["alt-x"])])).is_ok());
    for typed in ["Q", "x", "f2", "ctrl-]"] {
        assert!(
            matches!(
                Keymap::with(&set(&[("leave_tab", &[typed])])),
                Err(KeysError::InTab { .. })
            ),
            "{typed}"
        );
    }
}

#[test]
fn keys_every_screen_keeps_and_typed_keys_in_a_session_are_refused() {
    for key in ["enter", "up", "ctrl-c", "pagedown"] {
        assert!(
            matches!(
                Keymap::with(&set(&[("quit", &[key])])),
                Err(KeysError::Kept { .. })
            ),
            "{key}"
        );
    }
    let error = Keymap::with(&set(&[("leave_session", &["x"])])).unwrap_err();
    assert!(matches!(error, KeysError::Typed { .. }), "{error:?}");
    assert!(error.to_string().contains("needs ctrl or alt"), "{error}");
    assert!(Keymap::with(&set(&[("leave_session", &["alt-q"])])).is_ok());
}

#[test]
fn a_name_that_is_no_action_is_kept_aside_and_warned_of_never_refused() {
    // A plugin's action, its plugin removed: its key, whatever it is,
    // takes no other's place and makes nothing unreadable.
    let keys = Keymap::with(&set(&[
        ("zoetrope_graph", &["Z"]),
        ("open_sesame", &["not a key"]),
        ("rename", &["F2"]),
    ]))
    .unwrap();
    assert_eq!(keys.label(Action::Rename), "F2", "the rest as set");
    assert_eq!(
        keys.action(Screen::Sessions, Chord::parse("Z").unwrap()),
        None
    );
    assert_eq!(
        keys.aside().get("zoetrope_graph"),
        Some(&vec!["Z".to_owned()])
    );
    assert_eq!(
        keys.warnings(),
        [
            KeysWarning::NotAnAction("open_sesame".to_owned()),
            KeysWarning::NotAnAction("zoetrope_graph".to_owned()),
        ]
    );
    assert!(
        keys.warnings()[1]
            .to_string()
            .starts_with("keys.zoetrope_graph is not something a key does"),
        "{}",
        keys.warnings()[1]
    );
    assert!(Keymap::default().warnings().is_empty());
}

//! The keys' table for the desktop app (`ARC-24`): every action, the
//! screens it works on, its keys unless the person sets others, what it
//! does and where Settings › Keys lists it, and the keys every screen
//! keeps, written as `keys.json` beside the TypeScript bindings, so the
//! app's keymap reads Muster's own table rather than a copy of it. Only a
//! test behind the `ts` feature writes it, as `scripts/bindings.sh` runs
//! the bindings' (`export_bindings`).

use serde_json::{Value, json};

use super::{Action, Chord, Key, Screen, kept};

/// Every screen, in the order the help heads them.
const SCREENS: [Screen; 6] = [
    Screen::Sessions,
    Screen::NeedsYou,
    Screen::Tickets,
    Screen::Settings,
    Screen::Changes,
    Screen::InSession,
];

/// A screen's name in the table.
const fn id(screen: Screen) -> &'static str {
    match screen {
        Screen::Sessions => "sessions",
        Screen::NeedsYou => "needs_you",
        Screen::Tickets => "tickets",
        Screen::Settings => "settings",
        Screen::Changes => "changes",
        Screen::InSession => "in_session",
    }
}

/// The keys every screen keeps, as `[keys]` writes them, each with what
/// it is: every key with a name and every character, plain and with Ctrl,
/// Alt or both, asked of [`kept`].
fn kept_keys() -> Vec<Value> {
    let named = [
        Key::Tab,
        Key::BackTab,
        Key::Esc,
        Key::Backspace,
        Key::Enter,
        Key::Up,
        Key::Down,
        Key::Left,
        Key::Right,
        Key::PageUp,
        Key::PageDown,
        Key::Home,
        Key::End,
    ];
    let keys = named
        .into_iter()
        .chain((1..=24).map(Key::F))
        .chain((' '..='~').map(Key::Char));
    let mut out = Vec::new();
    for key in keys {
        for (ctrl, alt) in [(false, false), (true, false), (false, true), (true, true)] {
            let chord = Chord { key, ctrl, alt };
            if let Some(used) = kept(chord) {
                out.push(json!({ "key": chord.to_string(), "used": used }));
            }
        }
    }
    out
}

/// The table, as the app reads it.
fn table() -> Value {
    let screens: Vec<Value> = SCREENS
        .iter()
        .map(|screen| json!({ "id": id(*screen), "title": screen.title() }))
        .collect();
    let actions: Vec<Value> = Action::ALL
        .iter()
        .map(|action| {
            let screens: Vec<&str> = action.screens().iter().map(|screen| id(*screen)).collect();
            json!({
                "name": action.name(),
                "screens": screens,
                "keys": action.defaults(),
                "what": action.what(),
                "group": action.group(),
            })
        })
        .collect();
    json!({ "screens": screens, "actions": actions, "kept": kept_keys() })
}

#[test]
fn export_bindings_keys() {
    let table = table();
    // Every action's keys read, and none is one every screen keeps.
    for action in Action::ALL {
        for key in action.defaults() {
            let chord = Chord::parse(key).unwrap();
            assert_eq!(kept(chord), None, "{}: {key}", action.name());
        }
    }
    assert_eq!(
        table["actions"].as_array().map(Vec::len),
        Some(Action::ALL.len())
    );
    let kept: Vec<&str> = table["kept"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|kept| kept["key"].as_str())
        .collect();
    for key in ["enter", "up", "pagedown", "home", "ctrl-c"] {
        assert!(kept.contains(&key), "{key} is kept");
    }
    assert!(!kept.contains(&"q"), "q is an action's");
    if let Some(dir) = std::env::var_os("TS_RS_EXPORT_DIR") {
        let text = serde_json::to_string_pretty(&table).unwrap();
        std::fs::write(std::path::Path::new(&dir).join("keys.json"), text + "\n").unwrap();
    }
}

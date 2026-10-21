//! What a build for the end-to-end tests (`--features e2e`) records instead
//! of reaching outside the app: given `MUSTER_E2E_RECORD`, a folder, a web
//! address it is asked to open, a sound it is asked to play and a file it
//! is asked to show are each written there as a line (`opened`, `heard`,
//! `revealed`), and nothing is opened or played. A build without the
//! feature has no such way: [`recorded`] is always `false` there.

/// Records `what` under `kind` where the tests asked for it; whether it did.
#[cfg(feature = "e2e")]
pub(crate) fn recorded(kind: &str, what: &str) -> bool {
    use std::io::Write as _;
    let Some(folder) = std::env::var_os("MUSTER_E2E_RECORD").filter(|folder| !folder.is_empty())
    else {
        return false;
    };
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(std::path::Path::new(&folder).join(kind));
    if let Ok(mut file) = file {
        let _ = writeln!(file, "{what}");
    }
    true
}

/// A build without the `e2e` feature records nothing.
#[cfg(not(feature = "e2e"))]
pub(crate) fn recorded(_kind: &str, _what: &str) -> bool {
    false
}

/// Presses ⌘ and `letter` as the person's keyboard does: the key goes into
/// the app's own queue of events, and the Mac's menus take it as they take
/// the person's (⌘Q quits through the app menu's Quit). The embedded
/// `WebDriver` server makes its keys in the page, where no menu sees them.
#[cfg(feature = "e2e")]
#[tauri::command]
pub(crate) fn e2e_command_key(app: tauri::AppHandle, letter: String) -> Result<(), String> {
    use objc2_app_kit::{NSApplication, NSEvent, NSEventModifierFlags, NSEventType};
    use objc2_foundation::{MainThreadMarker, NSPoint, NSString};

    // The Mac's virtual key codes for the letters the tests press.
    let code = match letter.as_str() {
        "q" => 12,
        "w" => 13,
        _ => return Err(format!("no key code for {letter:?}")),
    };
    app.run_on_main_thread(move || {
        let Some(main) = MainThreadMarker::new() else {
            return;
        };
        let characters = NSString::from_str(&letter);
        let event = NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
            NSEventType::KeyDown,
            NSPoint::new(0.0, 0.0),
            NSEventModifierFlags::Command,
            0.0,
            0,
            None,
            &characters,
            &characters,
            false,
            code,
        );
        if let Some(event) = event {
            NSApplication::sharedApplication(main).postEvent_atStart(&event, false);
        }
    })
    .map_err(|error| error.to_string())
}

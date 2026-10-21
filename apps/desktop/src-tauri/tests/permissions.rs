//! Every command the app registers is one its window may call (`ARC-24`).
//! Once an app declares a permission for any command of its own, Tauri
//! refuses every command no permission in the window's capability allows,
//! and a refused `link_open` leaves the window waiting for the daemon for
//! good. The real app's tests (`pnpm e2e:app`) would see it, but they open a
//! window, which CI cannot; this reads the same three lists.

// Helpers outside `#[test]` functions are not covered by clippy.toml's test exemption.
#![expect(
    clippy::expect_used,
    reason = "a test reading the app's own files: a file not as expected is the test failing"
)]

use std::fs;
use std::path::Path;

/// The commands `lib.rs` registers, by name.
fn registered() -> Vec<String> {
    let source = include_str!("../src/lib.rs");
    let open = "generate_handler![";
    let start = source.find(open).expect("lib.rs registers its commands") + open.len();
    let end = start
        + source[start..]
            .find(']')
            .expect("the list of commands ends");
    source[start..end]
        .split(',')
        .filter_map(|entry| entry.trim().rsplit("::").next())
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect()
}

/// The quoted words in `text`, up to its first `]`.
fn quoted(text: &str) -> Vec<String> {
    let list = &text[..text.find(']').unwrap_or(text.len())];
    list.split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_owned)
        .collect()
}

/// The commands the permissions the main window's capability names allow.
fn allowed() -> Vec<String> {
    let capability = include_str!("../capabilities/default.json");
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("permissions");
    let mut allowed = Vec::new();
    for entry in fs::read_dir(dir).expect("the app's permissions") {
        let text = fs::read_to_string(entry.expect("a permissions file").path())
            .expect("a permissions file that reads");
        for permission in text.split("[[permission]]").skip(1) {
            let identifier = permission
                .split("identifier = \"")
                .nth(1)
                .and_then(|rest| rest.split('"').next())
                .expect("a permission names itself");
            if !capability.contains(&format!("\"{identifier}\"")) {
                continue;
            }
            if let Some(list) = permission.split("commands.allow").nth(1) {
                allowed.extend(quoted(list));
            }
        }
    }
    allowed
}

#[test]
fn arc_24_every_command_the_app_registers_is_allowed_to_its_window() {
    let allowed = allowed();
    let registered = registered();
    assert!(
        registered.contains(&"link_open".to_owned()),
        "{registered:?}"
    );
    for command in registered {
        assert!(
            allowed.contains(&command),
            "{command} is registered in lib.rs, but no permission that capabilities/default.json \
             names allows it: the window would be refused it"
        );
    }
}

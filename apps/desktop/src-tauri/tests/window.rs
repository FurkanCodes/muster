//! The window paints, before the page draws, the ground the page draws on
//! (`ARC-26`): a colour of its own would show as a frame of another colour
//! as the app opens.

#![expect(
    clippy::expect_used,
    reason = "a test reading the app's own files: a file not as expected is the test failing"
)]

/// The value of `name` in the dark theme's tokens, which the app opens in.
fn token(name: &str) -> String {
    let tokens = include_str!("../../src/styles/tokens.css");
    let line = tokens
        .lines()
        .find(|line| line.trim_start().starts_with(&format!("{name}:")))
        .expect("the token is defined, the dark theme first");
    line.split(':')
        .nth(1)
        .expect("a value")
        .trim()
        .trim_end_matches(';')
        .to_owned()
}

#[test]
fn the_window_s_background_is_the_page_s_ground() {
    let config: serde_json::Value =
        serde_json::from_str(include_str!("../tauri.conf.json")).expect("tauri.conf.json reads");
    let painted = config["app"]["windows"][0]["backgroundColor"]
        .as_str()
        .expect("the window names its background");
    assert_eq!(painted, token("--surface-ground"));
}

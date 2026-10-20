//! A plugin's hooks on the Settings screen's Plugins section and in its
//! preview (`PRD-15`, `ARCHITECTURE.md` §7a): each hook with the events it
//! runs for, its deadline, its filters and its command, so nobody approves
//! a hook they were not shown.

use crossterm::event::KeyCode;
use muster_core::plugins::{Manifest, PluginSource};
use muster_core::protocol::plugins::{PluginDetails, PluginInfo, PluginPreview, PreviewKind};
use muster_core::protocol::{Command, Delta, Reply, ServerFrame, TermSize};

use super::tests::{World, command, press, render_in, typed, world};
use super::{PluginKey, SettingRow, View};

const WIDE: TermSize = TermSize {
    cols: 160,
    rows: 44,
};

const MANIFEST: &str = r#"id = "notify"
name = "Notify"
version = "0.1.0"
description = "Tells a webhook when an agent needs you."
muster = "0.4.0"
requires = ["sh"]
secrets = ["WEBHOOK_URL"]

[[hooks]]
id = "ping"
on = ["session.needs_you", "session.finished"]
command = ["./notify", "{project}", "{branch}"]
timeout = 20
[hooks.when]
project = ["muster"]
harness = ["claude"]
away = true

[[hooks]]
id = "log"
on = ["worktree.created"]
command = ["./log"]
"#;

fn details() -> PluginDetails {
    PluginDetails::of(
        &Manifest::parse(MANIFEST).unwrap(),
        "ab12".repeat(16),
        |_| true,
    )
}

fn notify() -> PluginInfo {
    PluginInfo {
        id: "notify".to_owned(),
        source: PluginSource::Folder {
            path: "/src/notify".into(),
        },
        enabled: true,
        problem: None,
        details: Some(details()),
        changed: None,
    }
}

fn on_settings(selected: SettingRow) -> World {
    let mut world = world();
    world.app.on_resize(WIDE);
    world.app.view = View::Settings;
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::PluginsChanged(vec![notify()])));
    world.app.settings_selected = Some(selected);
    world
}

/// What a hook is shown as, wherever a plugin's details are.
const HOOK_LINES: [&str; 5] = [
    "hook ping on session.needs_you, session.finished, at most 20 s",
    "only project muster · harness claude · while you are away",
    "./notify {project} {branch}",
    "hook log on worktree.created, at most 10 s",
    "./log",
];

#[test]
fn a_plugin_s_line_names_its_hooks_and_its_details_show_each() {
    let world = on_settings(SettingRow::Plugin(PluginKey::of("notify").unwrap()));
    let screen = render_in(&world.app, WIDE);
    let line = screen
        .lines()
        .find(|line| line.contains("● Notify") || line.contains("● notify"))
        .unwrap_or_else(|| panic!("{screen}"));
    assert!(line.contains("hooks: ping, log"), "{line}");
    for said in HOOK_LINES {
        assert!(screen.contains(said), "{said}:\n{screen}");
    }
    assert!(screen.contains("It is given the secrets WEBHOOK_URL."));
}

#[test]
fn a_preview_shows_every_hook_before_the_person_says_yes() {
    let mut world = on_settings(SettingRow::Notch);
    press(&mut world.app, KeyCode::Char('a'));
    typed(&mut world.app, "https://example.com/notify.git");
    let (req_id, _) = command(&press(&mut world.app, KeyCode::Enter));
    let preview = PluginPreview {
        preview: "p1".to_owned(),
        kind: PreviewKind::Install,
        id: "notify".to_owned(),
        source: PluginSource::Git {
            url: "https://example.com/notify.git".to_owned(),
            git_ref: None,
            commit: "0123456789abcdef0123456789abcdef01234567".to_owned(),
        },
        details: details(),
        current: None,
    };
    world.app.on_frame(ServerFrame::Reply {
        req_id,
        outcome: Ok(Reply::PluginPreview(Box::new(preview))),
    });
    let screen = render_in(&world.app, WIDE);
    assert!(screen.contains(" Install notify? "), "{screen}");
    for said in HOOK_LINES {
        assert!(screen.contains(said), "{said}:\n{screen}");
    }
    assert!(matches!(
        command(&press(&mut world.app, KeyCode::Char('y'))).1,
        Command::Plugin(_)
    ));
}

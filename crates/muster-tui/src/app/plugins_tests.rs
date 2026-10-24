//! The Plugins section of the Settings screen (`PRODUCT.md` §6,
//! `ARCHITECTURE.md` §7a, `PRD-15`): each plugin with where it is from,
//! whether it is on and why not, and what it brings; its details beside
//! it; space turning one on or off, refused with why where it cannot run;
//! `a` adding one through its preview, `u` updating one the same way, `x`
//! removing one after asking; the list following the daemon's; and none
//! of it for a daemon before protocol 41.

use std::path::PathBuf;

use crossterm::event::KeyCode;
use muster_core::Timestamp;
use muster_core::plugins::{Manifest, PluginSource};
use muster_core::protocol::plugins::{
    ChangedBy, PluginAction, PluginChange, PluginCommand, PluginDetails, PluginInfo, PluginOrigin,
    PluginPreview, PluginProblem, PreviewKind,
};
use muster_core::protocol::{Command, Delta, ErrorBody, ErrorCode, Reply, ServerFrame, TermSize};

use super::tests::{World, command, press, render_in, typed, world};
use super::{Action, Dialog, Mode, PluginDialog, PluginKey, SettingRow, View};

/// A window wide enough for a plugin's whole line.
const WIDE: TermSize = TermSize {
    cols: 160,
    rows: 44,
};

const MANIFEST: &str = r#"id = "zoe"
name = "zoe"
version = "0.1.0"
description = "Watch a session as a graph."
muster = "0.3.0"
requires = ["sh", "zoe-not-on-path"]
how_to_get = "cargo install zoe"
env = ["ZOE_THEME"]
secrets = ["ZOE_TOKEN"]

[[panels]]
id = "graph"
title = "graph"
command = ["./graph", "{branch}", "two words"]
key = "Z"

[tickets]
name = "Zoe tickets"
noun = "task"
command = ["./tickets", "--serve"]
"#;

fn details(manifest: &str) -> PluginDetails {
    let manifest = Manifest::parse(manifest).unwrap();
    PluginDetails::of(&manifest, "ab12".repeat(16), |program| program == "sh")
}

fn git() -> PluginSource {
    PluginSource::Git {
        url: "https://example.com/zoe.git".to_owned(),
        git_ref: Some("main".to_owned()),
        commit: "0123456789abcdef0123456789abcdef01234567".to_owned(),
    }
}

/// The plugin zoe, from git, on.
fn zoe() -> PluginInfo {
    PluginInfo {
        id: "zoe".to_owned(),
        source: git(),
        enabled: true,
        problem: None,
        details: Some(details(MANIFEST)),
        changed: None,
    }
}

/// The plugin youtrack, on but changed since it was approved.
fn youtrack() -> PluginInfo {
    let manifest = MANIFEST
        .replace("id = \"zoe\"", "id = \"youtrack\"")
        .replace("name = \"zoe\"", "name = \"youtrack\"")
        .replace("0.1.0", "0.2.1");
    PluginInfo {
        id: "youtrack".to_owned(),
        source: PluginSource::Folder {
            path: PathBuf::from("/src/youtrack"),
        },
        enabled: true,
        problem: Some(PluginProblem::Changed),
        details: Some(details(&manifest)),
        changed: None,
    }
}

fn plugins(world: &mut World, plugins: Vec<PluginInfo>) {
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::PluginsChanged(plugins)));
}

/// The world on a wide Settings screen with zoe and youtrack, `selected`
/// selected.
fn on(selected: SettingRow) -> World {
    let mut world = world();
    world.app.on_resize(WIDE);
    world.app.view = View::Settings;
    plugins(&mut world, vec![zoe(), youtrack()]);
    world.app.settings_selected = Some(selected);
    world
}

fn key(id: &str) -> PluginKey {
    PluginKey::of(id).unwrap()
}

fn render(world: &World) -> String {
    render_in(&world.app, WIDE)
}

fn line<'a>(screen: &'a str, holding: &str) -> &'a str {
    screen
        .lines()
        .find(|line| line.contains(holding))
        .unwrap_or_else(|| panic!("no line holds {holding:?}:\n{screen}"))
}

fn reply(world: &mut World, req_id: u64, outcome: Result<Reply, ErrorBody>) {
    world.app.on_frame(ServerFrame::Reply { req_id, outcome });
}

fn refused(message: &str) -> Result<Reply, ErrorBody> {
    Err(ErrorBody {
        code: ErrorCode::BadRequest,
        message: message.to_owned(),
    })
}

fn plugin_command(actions: &[Action]) -> (u64, PluginCommand) {
    match command(actions) {
        (req_id, Command::Plugin(sent)) => (req_id, sent),
        (_, other) => panic!("{other:?}"),
    }
}

fn preview(kind: PreviewKind, current: Option<PluginInfo>) -> Box<PluginPreview> {
    Box::new(PluginPreview {
        preview: "p1".to_owned(),
        kind,
        id: "zoe".to_owned(),
        source: git(),
        details: PluginDetails {
            sha256: "cd34".repeat(16),
            ..details(&MANIFEST.replace("0.1.0", "0.2.0"))
        },
        current,
    })
}

#[test]
fn each_plugin_has_a_line_under_muster_s_own_saying_what_it_brings_or_why_it_is_off() {
    let world = on(SettingRow::Plugin(key("zoe")));
    let screen = render(&world);
    let rows = world.app.setting_rows();
    let at = rows
        .iter()
        .position(|row| *row == SettingRow::Plugins)
        .unwrap();
    assert!(
        rows.iter()
            .position(|row| *row == SettingRow::Muster)
            .unwrap()
            < at
    );
    assert_eq!(
        rows[at..],
        [
            SettingRow::Plugins,
            SettingRow::Plugin(key("zoe")),
            SettingRow::Plugin(key("youtrack")),
            SettingRow::AddPlugin
        ]
    );
    let zoe = line(&screen, "● zoe");
    for column in ["0.1.0", "git", " on ", "panel: graph (Z) · tickets"] {
        assert!(zoe.contains(column), "{column}: {zoe}");
    }
    let youtrack = line(&screen, "○ youtrack");
    for column in ["0.2.1", "folder", " off ", "changed since you approved it"] {
        assert!(youtrack.contains(column), "{column}: {youtrack}");
    }
    assert!(screen.contains("+ add a plugin…"), "{screen}");
    assert!(screen.contains("space turn off"), "{screen}");
    assert!(screen.contains("u update"), "{screen}");
    assert!(screen.contains("x remove"), "{screen}");
}

#[test]
fn the_details_say_everything_a_plugin_runs_needs_and_is_given() {
    let world = on(SettingRow::Plugin(key("zoe")));
    let screen = render(&world);
    for said in [
        "zoe 0.1.0",
        "Watch a session as a graph.",
        "On: it runs.",
        "git https://example.com/zoe.git at 0123456 (main)",
        "SHA-256       ab12ab12ab12…",
        "Muster 0.3.0 or later",
        "panel graph (beside a session, key Z)",
        "./graph {branch} 'two words'",
        "ticket source Zoe tickets",
        "./tickets --serve",
        "To get them: cargo install zoe",
        "It sees your variables ZOE_THEME.",
        "It is given the secrets ZOE_TOKEN.",
    ] {
        assert!(screen.contains(said), "{said}:\n{screen}");
    }
    assert!(line(&screen, "zoe-not-on-path").contains("not found"));
    assert!(line(&screen, "  sh  ").contains("found"));

    let world = on(SettingRow::Plugin(key("youtrack")));
    let screen = render(&world);
    assert!(
        screen.contains("Off until you approve it again: its manifest changed since you"),
        "{screen}"
    );
    assert!(
        screen.contains("the folder /src/youtrack, read where it is"),
        "{screen}"
    );
}

#[test]
fn a_change_made_from_the_command_line_is_marked_with_when() {
    let mut world = on(SettingRow::Plugin(key("zoe")));
    let at = Timestamp::from_millis(1_000_000);
    world.app.now = Some(Timestamp::from_millis(1_000_000 + 125_000));
    let mut changed = zoe();
    changed.changed = Some(PluginChange {
        what: PluginAction::TurnedOn,
        by: ChangedBy::CommandLine,
        at,
    });
    plugins(&mut world, vec![changed.clone()]);
    let screen = render(&world);
    // The line says when where it has room; its details always do.
    assert!(
        line(&screen, "● zoe").contains("turned on from the command line"),
        "{screen}"
    );
    assert!(
        screen.contains("Turned on 2 min ago from the command line."),
        "{screen}"
    );

    // One made here is said in its details only.
    changed.changed = Some(PluginChange {
        by: ChangedBy::Ui,
        ..changed.changed.unwrap()
    });
    plugins(&mut world, vec![changed]);
    let screen = render(&world);
    assert!(line(&screen, "● zoe").contains("panel: graph"), "{screen}");
    assert!(
        screen.contains("Turned on 2 min ago on the Settings screen."),
        "{screen}"
    );
}

#[test]
fn space_turns_a_plugin_off_and_on_waiting_for_each_answer() {
    let mut world = on(SettingRow::Plugin(key("zoe")));
    let (req_id, sent) = plugin_command(&press(&mut world.app, KeyCode::Char(' ')));
    assert_eq!(
        sent,
        PluginCommand::SetEnabled {
            id: "zoe".to_owned(),
            on: false,
            by: ChangedBy::Ui
        }
    );
    assert_eq!(press(&mut world.app, KeyCode::Char(' ')), [], "waits");
    assert!(line(&render(&world), "zoe ").contains("…"));
    let mut off = zoe();
    off.enabled = false;
    plugins(&mut world, vec![off, youtrack()]);
    reply(&mut world, req_id, Ok(Reply::Done));
    let screen = render(&world);
    assert!(screen.contains("Turned zoe off."), "{screen}");
    assert!(line(&screen, "○ zoe").contains(" off "), "{screen}");
    world.app.note = None;
    assert!(render(&world).contains("space turn on"));

    // ⏎ does as space does.
    let (req_id, sent) = plugin_command(&press(&mut world.app, KeyCode::Enter));
    assert!(
        matches!(sent, PluginCommand::SetEnabled { on: true, .. }),
        "{sent:?}"
    );
    reply(&mut world, req_id, refused("zoe cannot be turned on: no."));
    assert!(render(&world).contains("zoe cannot be turned on: no."));
    assert_eq!(world.app.plugin_changing, None);
}

#[test]
fn a_plugin_that_cannot_run_is_not_turned_on_and_the_person_is_told_why() {
    let mut world = on(SettingRow::Plugin(key("youtrack")));
    assert_eq!(press(&mut world.app, KeyCode::Char(' ')), []);
    let screen = render(&world);
    assert!(
        screen.contains(
            "youtrack cannot be turned on: its manifest changed since you approved it: update it"
        ),
        "{screen}"
    );
    assert!(!screen.contains("u update   x remove   space"), "{screen}");
}

#[test]
fn a_plugin_is_added_from_git_after_its_preview_and_selected_once_listed() {
    let mut world = on(SettingRow::Notch);
    assert_eq!(press(&mut world.app, KeyCode::Char('a')), []);
    assert!(render(&world).contains("A git repository's address"));
    typed(&mut world.app, "https://example.com/zoe.git");
    press(&mut world.app, KeyCode::Tab);
    typed(&mut world.app, "main");
    let (req_id, sent) = plugin_command(&press(&mut world.app, KeyCode::Enter));
    assert_eq!(
        sent,
        PluginCommand::Preview {
            origin: PluginOrigin::Git {
                url: "https://example.com/zoe.git".to_owned(),
                git_ref: Some("main".to_owned()),
            }
        }
    );
    let screen = render(&world);
    assert!(
        screen.contains("Fetching the plugin") && screen.contains("https://example.com/zoe.git"),
        "{screen}"
    );
    assert!(screen.contains("esc give up"), "{screen}");

    reply(
        &mut world,
        req_id,
        Ok(Reply::PluginPreview(preview(PreviewKind::Install, None))),
    );
    let screen = render(&world);
    for said in [
        " Install zoe? ",
        "zoe 0.2.0 (zoe)",
        "Muster's data folder, plugins/zoe/0123456/, yours alone",
        "./graph {branch} 'two words'",
        "./tickets --serve",
        "It needs programs that are not on PATH",
        "It is given the secrets ZOE_TOKEN.",
        "SHA-256       cd34cd34cd34cd34cd34",
        "y install   n no",
    ] {
        assert!(screen.contains(said), "{said}:\n{screen}");
    }

    plugins(&mut world, vec![youtrack()]);
    let (req_id, sent) = plugin_command(&press(&mut world.app, KeyCode::Char('y')));
    assert_eq!(
        sent,
        PluginCommand::Install {
            preview: "p1".to_owned(),
            by: ChangedBy::Ui
        }
    );
    assert_eq!(world.app.mode, Mode::Browse);
    assert!(render(&world).contains("Installing zoe…"));
    reply(&mut world, req_id, Ok(Reply::Done));
    assert!(render(&world).contains("Installed zoe and turned it on."));
    // The change to the state can come after the reply.
    plugins(&mut world, vec![zoe(), youtrack()]);
    assert_eq!(
        world.app.settings_selected,
        Some(SettingRow::Plugin(key("zoe")))
    );
}

#[test]
fn a_folder_is_linked_and_an_address_refused_is_said_in_the_dialog_kept_as_typed() {
    let mut world = on(SettingRow::AddPlugin);
    press(&mut world.app, KeyCode::Enter);
    world.app.on_paste("/src/my-plugin");
    assert!(
        render(&world).contains("Branch or tag: none, a folder is read as it is."),
        "{}",
        render(&world)
    );
    let (req_id, sent) = plugin_command(&press(&mut world.app, KeyCode::Enter));
    assert_eq!(
        sent,
        PluginCommand::Preview {
            origin: PluginOrigin::Folder {
                path: PathBuf::from("/src/my-plugin")
            }
        }
    );
    reply(
        &mut world,
        req_id,
        refused("/src/my-plugin holds no muster-plugin.toml"),
    );
    let screen = render(&world);
    assert!(
        screen.contains("/src/my-plugin holds no muster-plugin.toml"),
        "{screen}"
    );
    let Mode::Dialog(Dialog::Plugin(PluginDialog::Add { typed, .. })) = &world.app.mode else {
        panic!("{:?}", world.app.mode);
    };
    assert_eq!(typed.address.text(), "/src/my-plugin");

    // Nothing typed is refused before anything is asked.
    press(&mut world.app, KeyCode::Home);
    for _ in 0.."/src/my-plugin".len() {
        press(&mut world.app, KeyCode::Delete);
    }
    assert_eq!(press(&mut world.app, KeyCode::Enter), []);
    assert!(render(&world).contains("Type a git repository's address, or a folder's path."));
    press(&mut world.app, KeyCode::Esc);
    assert_eq!(world.app.mode, Mode::Browse);
}

#[test]
fn esc_gives_up_waiting_and_a_preview_coming_after_is_let_go() {
    let mut world = on(SettingRow::AddPlugin);
    press(&mut world.app, KeyCode::Char('a'));
    typed(&mut world.app, "https://example.com/zoe.git");
    let (req_id, _) = plugin_command(&press(&mut world.app, KeyCode::Enter));
    press(&mut world.app, KeyCode::Esc);
    assert_eq!(world.app.mode, Mode::Browse);
    assert!(render(&world).contains("Stopped waiting for https://example.com/zoe.git."));
    reply(
        &mut world,
        req_id,
        Ok(Reply::PluginPreview(preview(PreviewKind::Install, None))),
    );
    assert_eq!(world.app.mode, Mode::Browse);
}

#[test]
fn no_on_the_preview_adds_nothing() {
    let mut world = on(SettingRow::AddPlugin);
    world.app.mode = Mode::Dialog(Dialog::Plugin(PluginDialog::Preview {
        preview: preview(PreviewKind::Install, None),
        offset: 0,
    }));
    assert_eq!(press(&mut world.app, KeyCode::Char('n')), []);
    assert_eq!(world.app.mode, Mode::Browse);
    assert!(render(&world).contains("Nothing was added."));
}

#[test]
fn a_long_preview_scrolls_to_its_last_line_and_no_further() {
    let mut world = on(SettingRow::AddPlugin);
    world.app.on_resize(TermSize {
        cols: 100,
        rows: 20,
    });
    let shown = preview(PreviewKind::Install, None);
    let lines = crate::view::preview_lines(&shown, crate::view::theme::of(&world.app)).len();
    world.app.mode = Mode::Dialog(Dialog::Plugin(PluginDialog::Preview {
        preview: shown,
        offset: 0,
    }));
    let size = TermSize {
        cols: 100,
        rows: 20,
    };
    let screen = render_in(&world.app, size);
    assert!(screen.contains("zoe 0.2.0 (zoe)"), "{screen}");
    assert!(screen.contains("↑↓ PgUp PgDn more"), "{screen}");
    assert!(!screen.contains("Adding it turns it on."), "{screen}");
    press(&mut world.app, KeyCode::End);
    let Mode::Dialog(Dialog::Plugin(PluginDialog::Preview { offset, .. })) = &world.app.mode else {
        panic!();
    };
    assert_eq!(*offset, lines - 1);
    let screen = render_in(&world.app, size);
    assert!(screen.contains("Adding it turns it on."), "{screen}");
    press(&mut world.app, KeyCode::PageUp);
    press(&mut world.app, KeyCode::Home);
    assert!(render_in(&world.app, size).contains("zoe 0.2.0 (zoe)"));
}

#[test]
fn u_updates_a_plugin_after_showing_what_it_runs_now() {
    let mut world = on(SettingRow::Plugin(key("zoe")));
    let (req_id, sent) = plugin_command(&press(&mut world.app, KeyCode::Char('u')));
    assert_eq!(
        sent,
        PluginCommand::Update {
            id: "zoe".to_owned()
        }
    );
    assert!(render(&world).contains("zoe as it is now"));
    reply(
        &mut world,
        req_id,
        Ok(Reply::PluginPreview(preview(
            PreviewKind::Update,
            Some(zoe()),
        ))),
    );
    let screen = render(&world);
    assert!(screen.contains(" Update zoe? "), "{screen}");
    assert!(
        screen.contains("0.1.0 at 0123456, with a manifest that is not this one"),
        "{screen}"
    );
    assert!(
        screen.contains("Updating keeps it on or off, as it is."),
        "{screen}"
    );
    let (req_id, sent) = plugin_command(&press(&mut world.app, KeyCode::Enter));
    assert!(matches!(sent, PluginCommand::Install { .. }), "{sent:?}");
    reply(&mut world, req_id, Ok(Reply::Done));
    assert!(render(&world).contains("Updated zoe to 0.2.0."));

    // A fetch that fails is said, and changes nothing.
    let (req_id, _) = plugin_command(&press(&mut world.app, KeyCode::Char('u')));
    reply(
        &mut world,
        req_id,
        refused("git could not fetch it: not found"),
    );
    let screen = render(&world);
    assert!(screen.contains(" Update zoe as it is now "), "{screen}");
    assert!(
        screen.contains("git could not fetch it: not found"),
        "{screen}"
    );
    press(&mut world.app, KeyCode::Esc);
    assert_eq!(world.app.mode, Mode::Browse);
}

#[test]
fn x_removes_a_plugin_after_asking_and_muster_s_own_only_turn_off() {
    let mut world = on(SettingRow::Plugin(key("zoe")));
    assert_eq!(press(&mut world.app, KeyCode::Char('x')), []);
    assert!(
        render(&world)
            .contains("Remove zoe? Muster's copy of it and its entry in your settings go.")
    );
    let (req_id, sent) = plugin_command(&press(&mut world.app, KeyCode::Char('y')));
    assert_eq!(
        sent,
        PluginCommand::Remove {
            id: "zoe".to_owned()
        }
    );
    plugins(&mut world, vec![youtrack()]);
    reply(&mut world, req_id, Ok(Reply::Done));
    assert!(render(&world).contains("Removed zoe."));
    assert_eq!(world.app.settings_selected, Some(SettingRow::AddPlugin));

    // A linked folder is left where it is.
    world.app.settings_selected = Some(SettingRow::Plugin(key("youtrack")));
    press(&mut world.app, KeyCode::Char('x'));
    let screen = render(&world);
    assert!(
        screen.contains("Remove youtrack? Muster forgets it; its folder /src/youtrack stays as"),
        "{screen}"
    );
    press(&mut world.app, KeyCode::Char('n'));

    let mut own = zoe();
    own.source = PluginSource::Builtin;
    plugins(&mut world, vec![own]);
    world.app.settings_selected = Some(SettingRow::Plugin(key("zoe")));
    assert_eq!(press(&mut world.app, KeyCode::Char('x')), []);
    assert!(render(&world).contains("zoe is Muster's own: it cannot be removed, only turned off."));
    assert_eq!(press(&mut world.app, KeyCode::Char('u')), []);
    assert!(render(&world).contains("zoe is Muster's own: it updates with Muster."));

    // Off a plugin, neither does anything but say so.
    world.app.settings_selected = Some(SettingRow::Notch);
    assert_eq!(press(&mut world.app, KeyCode::Char('x')), []);
    assert!(render(&world).contains("Select a plugin to remove it."));
}

#[test]
fn showing_the_settings_has_the_daemon_read_its_plugins_again() {
    let mut world = world();
    let actions = world.app.show(View::Settings);
    let sent: Vec<&Command> = actions
        .iter()
        .filter_map(|action| match action {
            Action::Send(muster_core::protocol::ClientFrame::Command { command, .. }) => {
                Some(command)
            }
            _ => None,
        })
        .collect();
    assert!(
        sent.contains(&&Command::Plugin(PluginCommand::List)),
        "{sent:?}"
    );
}

#[test]
fn the_section_says_what_plugins_are_and_its_keys_follow_the_person_s() {
    let mut world = on(SettingRow::Plugins);
    let screen = render(&world);
    assert!(
        screen.contains("Plugins bring panels beside a session"),
        "{screen}"
    );
    assert!(screen.contains("a add a plugin"), "{screen}");
    super::tests::preferences(&mut world.app, |preferences| {
        preferences
            .keys
            .insert("add_plugin".to_owned(), vec!["A".to_owned()]);
    });
    let screen = render(&world);
    assert!(screen.contains("Shift-A adds one"), "{screen}");
    assert_eq!(press(&mut world.app, KeyCode::Char('a')), []);
    assert_eq!(world.app.mode, Mode::Browse);
}

#[test]
fn a_long_address_wraps_to_rows_of_its_own_and_the_preview_still_ends_with_its_keys() {
    let mut world = on(SettingRow::AddPlugin);
    let mut shown = preview(PreviewKind::Install, None);
    shown.source = PluginSource::Git {
        url: format!(
            "file:///private/tmp/{}/fixtures/zoetrope",
            "a-long-folder".repeat(9)
        ),
        git_ref: None,
        commit: "59a2c23".repeat(5),
    };
    world.app.mode = Mode::Dialog(Dialog::Plugin(PluginDialog::Preview {
        preview: shown,
        offset: 0,
    }));
    let screen = render(&world);
    assert!(screen.contains("Adding it turns it on."), "{screen}");
    assert!(screen.contains("y install   n no"), "{screen}");
    assert!(!screen.contains("more"), "{screen}");
}

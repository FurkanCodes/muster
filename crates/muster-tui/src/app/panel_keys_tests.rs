//! A plugin's panel by its key (`PRODUCT.md` §6, `PRD-15`): the key a
//! running plugin's manifest suggests opens its panel beside the session
//! selected, or on the line's workspace, and again puts it away; the menu
//! a right-click opens lists the line's panels with their keys, the help
//! lists them, and Settings › Keys sets them under the plugin's name. A
//! daemon older than panels is asked to open none, and kept a layout
//! without one.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use muster_core::plugins::{Manifest, PluginSource};
use muster_core::protocol::plugins::{Beside, PluginCommand, PluginDetails, PluginInfo};
use muster_core::protocol::{Command, Delta, ServerFrame, TermSize};
use muster_core::settings::PreferenceChange;
use muster_core::{Session, SessionStatus};

use super::tests::{World, command, key, world};
use super::{Action, Help, HelpLine, Menu, Mode, RebindRow, Rebinding, Rebound};
use crate::model::Row;
use crate::model::tests::session;

const MANIFEST: &str = r#"id = "zoe"
name = "Zoe"
version = "0.1.0"
muster = "0.3.0"

[[panels]]
id = "graph"
title = "graph"
on = "session"
command = ["zoe", "{transcript}"]
key = "Z"

[[panels]]
id = "files"
title = "files"
on = "workspace"
command = ["zoe", "{workspace}"]
key = "G"
"#;

const GRAPH: &str = "plugin.zoe.graph";

/// The world, wide, its session selected, with the plugin zoe running.
fn with_zoe() -> World {
    let mut world = world();
    world.app.selected = Some(Row::Session(world.session.id));
    world.app.on_resize(TermSize {
        cols: 180,
        rows: 29,
    });
    let manifest = Manifest::parse(MANIFEST).unwrap();
    let zoe = PluginInfo {
        id: "zoe".to_owned(),
        source: PluginSource::Folder {
            path: "/src/zoe".into(),
        },
        enabled: true,
        problem: None,
        details: Some(PluginDetails::of(&manifest, "ab".repeat(32), |_| true)),
        changed: None,
    };
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::PluginsChanged(vec![zoe])));
    world
}

fn shift(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::SHIFT)
}

/// The panel asked for in `actions`: its plugin, panel and where.
fn opened(actions: &[Action]) -> (String, String, Beside) {
    match command(actions).1 {
        Command::Plugin(PluginCommand::OpenPanel {
            plugin,
            panel,
            beside,
            ..
        }) => (plugin, panel, beside),
        other => panic!("unexpected {other:?}"),
    }
}

/// zoe's `panel` running in the world's worktree, beside `beside`.
fn running(world: &mut World, panel: &str, beside: Option<&Session>) -> Session {
    let mut running = session(world.worktree.id, panel, 7);
    running.harness = format!("plugin:zoe/{panel}");
    running.status = SessionStatus::Running;
    running.beside = beside.map(|chat| chat.id);
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::SessionUpserted(running.clone())));
    running
}

#[test]
fn its_key_opens_the_panel_beside_the_session_selected_and_again_puts_it_away() {
    let mut world = with_zoe();
    let chat = world.session.clone();
    let actions = world.app.on_key(shift('Z'));
    assert_eq!(
        opened(&actions),
        (
            "zoe".to_owned(),
            "graph".to_owned(),
            Beside::Session(chat.id)
        )
    );
    // Shown beside the chat once the state has it; the keys stay put.
    let graph = running(&mut world, "graph", Some(&chat));
    assert_eq!(world.app.plugin_beside(), Some(graph.id));
    assert_eq!(world.app.mode, Mode::Browse);
    // Again: put away, its program ended.
    let actions = world.app.on_key(shift('Z'));
    assert_eq!(
        command(&actions).1,
        Command::StopSession { session: graph.id }
    );
    assert!(world.app.closing.contains(&graph.id));
}

#[test]
fn a_workspace_s_panel_opens_on_the_line_s_workspace_and_a_session_s_needs_a_session() {
    let mut world = with_zoe();
    world.app.selected = Some(Row::Worktree(world.worktree.id));
    let actions = world.app.on_key(shift('G'));
    assert_eq!(opened(&actions).2, Beside::Workspace(world.worktree.id));
    let actions = world.app.on_key(shift('Z'));
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Send(_))),
        "{actions:?}"
    );
    let said = world.app.note.as_ref().map(|note| note.text.clone());
    assert_eq!(
        said.as_deref(),
        Some("Select a session to open graph beside it.")
    );
    // A workspace's panel shows beside each of its lines with none of
    // their own.
    let files = running(&mut world, "files", None);
    assert_eq!(world.app.plugin_beside(), Some(files.id));
    world.app.selected = Some(Row::Session(world.session.id));
    assert_eq!(world.app.plugin_beside(), Some(files.id));
}

#[test]
fn a_panel_beside_another_chat_is_not_shown_beside_this_one() {
    let mut world = with_zoe();
    let mut other = session(world.worktree.id, "Other chat", 5);
    other.harness = "claude".to_owned();
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::SessionUpserted(other.clone())));
    let theirs = running(&mut world, "graph", Some(&other));
    assert_eq!(
        world.app.plugin_beside(),
        None,
        "the chat selected has none"
    );
    world.app.selected = Some(Row::Session(other.id));
    assert_eq!(world.app.plugin_beside(), Some(theirs.id));
}

#[test]
fn the_menu_lists_the_line_s_panels_with_their_keys_and_choosing_one_opens_it() {
    let mut world = with_zoe();
    let items = world.app.actions_here();
    let panels: Vec<(String, String)> = items
        .iter()
        .filter(|item| item.panel.is_some())
        .map(|item| (item.key.clone(), item.what.clone()))
        .collect();
    assert_eq!(
        panels,
        [
            ("Shift-Z".to_owned(), "graph".to_owned()),
            ("Shift-G".to_owned(), "files".to_owned())
        ]
    );
    let chosen = items
        .iter()
        .position(|item| item.panel.as_deref() == Some(GRAPH))
        .unwrap();
    let menu = Menu {
        at: (2, 3),
        items,
        chosen,
    };
    let actions = world.app.menu_key(menu, &key(KeyCode::Enter));
    assert_eq!(opened(&actions).1, "graph");
    // On a worktree's line, only the workspace's panel.
    world.app.selected = Some(Row::Worktree(world.worktree.id));
    let on_worktree: Vec<String> = world
        .app
        .actions_here()
        .into_iter()
        .filter_map(|item| item.panel)
        .collect();
    assert_eq!(on_worktree, ["plugin.zoe.files"]);
}

#[test]
fn the_help_lists_the_panels_by_their_keys() {
    let world = with_zoe();
    let lines = Help::default().lines(super::View::Sessions, &world.app.keymap);
    let at = lines
        .iter()
        .position(|line| *line == HelpLine::Heading("Plugins' panels"))
        .expect("a heading of their own");
    assert_eq!(
        lines[at + 1],
        HelpLine::Key(
            "Shift-Z".to_owned(),
            "graph beside the session (again: put it away)".to_owned()
        )
    );
    // Not on the Tickets screen, where no panel opens.
    let tickets = Help::default().lines(super::View::Tickets, &world.app.keymap);
    assert!(!tickets.contains(&HelpLine::Heading("Plugins' panels")));
}

#[test]
fn settings_keys_lists_a_panel_under_its_plugin_and_sets_its_key() {
    let mut world = with_zoe();
    let rows = world.app.rebind_rows();
    let at = rows
        .iter()
        .position(|row| *row == RebindRow::Heading("Zoe".to_owned()))
        .expect("under its plugin's name");
    assert_eq!(
        rows[at + 1],
        RebindRow::Key(Rebound::Panel(GRAPH.to_owned()))
    );
    let chosen = rows[..=at + 1]
        .iter()
        .filter(|row| matches!(row, RebindRow::Key(_)))
        .count()
        - 1;
    let rebinding = Rebinding {
        chosen,
        ..Rebinding::default()
    };
    assert_eq!(
        world.app.rebinding_chosen(&rebinding),
        Rebound::Panel(GRAPH.to_owned())
    );
    let waiting = Rebinding {
        waiting: true,
        ..rebinding.clone()
    };
    // A key Muster has on the lists of sessions is refused.
    let actions = world.app.rebind_key(waiting.clone(), &shift('T'));
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Send(_)))
    );
    let said = world.app.note.as_ref().map(|note| note.text.clone());
    assert!(said.unwrap_or_default().contains("is shell's"));
    let actions = world.app.rebind_key(waiting, &shift('Y'));
    assert_eq!(
        command(&actions).1,
        Command::ChangePreference(PreferenceChange::Keys {
            action: GRAPH.to_owned(),
            keys: Some(vec!["Y".to_owned()]),
        })
    );
}

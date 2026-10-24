//! The Settings screen: reading each project's settings as it is shown,
//! turning only-my-tickets on and off, and what happens to the tickets.

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use muster_core::ProjectId;
use muster_core::protocol::{ClientFrame, Command, ErrorBody, ErrorCode, Reply, ServerFrame};
use muster_core::settings::{ProjectSettings, SettingChange, SettingSource, Toggle};

use super::tests::{World, command, press, render, world};
use super::{Action, Mode, SettingRow, Settings, Tickets, View};
use muster_core::telling::Kind;

pub(super) fn settings(on: bool, from: SettingSource) -> ProjectSettings {
    ProjectSettings {
        tickets_from: "GitHub issues, as its git remote says".to_owned(),
        only_mine: Toggle { on, from },
        file: PathBuf::from("/home/me/.config/muster/projects/p.toml"),
        tickets: None,
        tickets_set_by: SettingSource::Default,
        tickets_untold: false,
        muster_prefix: Some(Toggle {
            on: true,
            from: SettingSource::Default,
        }),
    }
}

pub(super) fn reply(
    world: &mut World,
    req_id: u64,
    outcome: Result<Reply, ErrorBody>,
) -> Vec<Action> {
    world.app.on_frame(ServerFrame::Reply { req_id, outcome })
}

fn commands(actions: &[Action]) -> Vec<Command> {
    actions
        .iter()
        .filter_map(|action| match action {
            Action::Send(ClientFrame::Command { command, .. }) => Some(command.clone()),
            _ => None,
        })
        .collect()
}

/// The world on the Settings screen, its project's settings read: only
/// my tickets `on` or not, set by nothing.
pub(super) fn on_settings(on: bool) -> World {
    let mut world = world();
    let actions = press(&mut world.app, KeyCode::Char('4'));
    assert_eq!(world.app.view, View::Settings);
    let (req_id, asked) = command(&actions);
    assert_eq!(
        asked,
        Command::ProjectSettings {
            project: world.project.id
        }
    );
    assert!(render(&world.app).contains("app  reading…"));
    reply(
        &mut world,
        req_id,
        Ok(Reply::ProjectSettings(settings(on, SettingSource::Default))),
    );
    world
}

fn toggle_asked(actions: &[Action], project: ProjectId, on: bool) -> u64 {
    let (req_id, asked) = command(actions);
    assert_eq!(
        asked,
        Command::ChangeSetting {
            project,
            change: SettingChange::OnlyMyTickets(on),
        }
    );
    req_id
}

#[test]
fn the_screen_reads_each_project_s_settings_and_starts_on_the_first() {
    let world = on_settings(false);
    let project = world.project.id;
    assert_eq!(
        world.app.setting_rows(),
        [
            SettingRow::Project(project),
            SettingRow::OnlyMine(project),
            SettingRow::TicketsFrom(project),
            SettingRow::MusterPrefix(project),
            SettingRow::Muster,
            SettingRow::Notifications,
            SettingRow::Tell(Kind::Question),
            SettingRow::Tell(Kind::Finish),
            SettingRow::Tell(Kind::Error),
            SettingRow::SideBySide,
            SettingRow::Colours,
            SettingRow::SessionLines,
            SettingRow::Panels,
            SettingRow::NamesByAgent,
            SettingRow::NamedBy,
            SettingRow::NamingModel,
            SettingRow::Keys,
            SettingRow::SharedBuilds,
            SettingRow::Delegation,
            SettingRow::Agents,
            SettingRow::Plugins,
            SettingRow::AddPlugin,
        ]
    );
    // The first setting is selected, so ⏎ does something at once.
    assert_eq!(
        world.app.settings_selected,
        Some(SettingRow::OnlyMine(project))
    );
    let screen = render(&world.app);
    assert!(screen.contains("Only my tickets"), "{screen}");
    assert!(screen.contains("off"), "{screen}");
    assert!(
        screen.contains("The Tickets screen lists every open ticket."),
        "{screen}"
    );
    assert!(
        screen.contains("Tickets from  GitHub issues, as its git remote says"),
        "{screen}"
    );
    assert!(
        screen.contains("Set by        nothing yet: off unless set"),
        "{screen}"
    );
    assert!(screen.contains("projects/p.toml"), "{screen}");
    assert!(screen.contains("⏎ or space turns it on"), "{screen}");
}

#[test]
fn enter_turns_it_on_and_the_tickets_are_fetched_again_as_chosen() {
    let mut world = world();
    let project = world.project.id;
    // The tickets were fetched before, every one of them.
    let actions = press(&mut world.app, KeyCode::Char('3'));
    let (req_id, _) = command(&actions);
    reply(
        &mut world,
        req_id,
        Ok(Reply::Issues {
            source: None,
            issues: Vec::new(),
            tracker: "GitHub issues".to_owned(),
            mine: false,
        }),
    );
    assert!(render(&world.app).contains("app  no open tickets"));
    let actions = press(&mut world.app, KeyCode::Char('4'));
    let (req_id, _) = command(&actions);
    reply(
        &mut world,
        req_id,
        Ok(Reply::ProjectSettings(settings(
            false,
            SettingSource::Default,
        ))),
    );

    let actions = press(&mut world.app, KeyCode::Enter);
    let req_id = toggle_asked(&actions, project, true);
    // Asked once: pressed again before the answer, nothing more is sent.
    assert!(commands(&press(&mut world.app, KeyCode::Enter)).is_empty());
    let value_at_end = |value: &str| {
        // The list's 38 columns past the project's edge, the value at
        // their end.
        let label = "  Only my tickets";
        format!(
            "{label}{}{value}",
            " ".repeat(37 - label.len() - value.chars().count())
        )
    };
    assert!(render(&world.app).contains(&value_at_end("…")));
    reply(
        &mut world,
        req_id,
        Ok(Reply::ProjectSettings(settings(true, SettingSource::Yours))),
    );
    let screen = render(&world.app);
    assert!(
        screen.contains("Tickets for app now lists only the tickets assigned to you."),
        "{screen}"
    );
    assert!(screen.contains(&value_at_end("on")), "{screen}");
    assert!(screen.contains("Set by        your settings"), "{screen}");
    assert!(!world.app.tickets.contains_key(&project));

    // The Tickets screen fetches them again, and says they are yours.
    let actions = press(&mut world.app, KeyCode::Char('3'));
    let (req_id, asked) = command(&actions);
    assert_eq!(asked, Command::ListIssues { project });
    reply(
        &mut world,
        req_id,
        Ok(Reply::Issues {
            source: None,
            issues: Vec::new(),
            tracker: "GitHub issues".to_owned(),
            mine: true,
        }),
    );
    let screen = render(&world.app);
    assert!(screen.contains("app  none of yours open"), "{screen}");
    assert!(
        screen.contains("Its open tickets assigned to you from GitHub issues"),
        "{screen}"
    );
    assert!(
        screen.contains("Settings (4) can list every open ticket"),
        "{screen}"
    );
}

#[test]
fn space_turns_it_off_again() {
    let mut world = on_settings(true);
    assert!(render(&world.app).contains("⏎ turn off"));
    let actions = press(&mut world.app, KeyCode::Char(' '));
    let req_id = toggle_asked(&actions, world.project.id, false);
    reply(
        &mut world,
        req_id,
        Ok(Reply::ProjectSettings(settings(
            false,
            SettingSource::Yours,
        ))),
    );
    assert!(
        render(&world.app).contains("Tickets for app now lists every open ticket."),
        "{}",
        render(&world.app)
    );
}

#[test]
fn a_refused_change_is_said_and_the_settings_are_read_again() {
    let mut world = on_settings(false);
    let project = world.project.id;
    let actions = press(&mut world.app, KeyCode::Enter);
    let req_id = toggle_asked(&actions, project, true);
    let actions = reply(
        &mut world,
        req_id,
        Err(ErrorBody::new(
            ErrorCode::BadRequest,
            "/home/me/.config/muster/projects/p.toml: unknown field `mien`",
        )),
    );
    assert!(world.app.note.as_ref().is_some_and(|note| note.error));
    assert!(render(&world.app).contains("unknown field `mien`"));
    // As they still are, in case the file changed.
    assert_eq!(commands(&actions), [Command::ProjectSettings { project }]);
}

#[test]
fn settings_that_cannot_be_read_say_why_and_are_not_changed() {
    let mut world = world();
    let project = world.project.id;
    let actions = press(&mut world.app, KeyCode::Char('4'));
    let (req_id, _) = command(&actions);
    reply(
        &mut world,
        req_id,
        Err(ErrorBody::new(
            ErrorCode::BadRequest,
            "muster.toml is not valid TOML: expected `=`",
        )),
    );
    let screen = render(&world.app);
    assert!(screen.contains("app  cannot be read"), "{screen}");
    assert!(screen.contains("Its settings cannot be read:"), "{screen}");
    assert!(screen.contains("expected `=`"), "{screen}");
    assert!(commands(&press(&mut world.app, KeyCode::Enter)).is_empty());
    assert!(
        world
            .app
            .note
            .as_ref()
            .is_some_and(|note| note.error && note.text.contains("expected `=`"))
    );
    assert!(matches!(
        world.app.settings.get(&project),
        Some(Settings::Failed(_))
    ));
}

#[test]
fn settings_are_read_afresh_each_time_the_screen_is_shown_and_on_r() {
    let mut world = on_settings(false);
    let project = world.project.id;
    press(&mut world.app, KeyCode::Char('1'));
    let actions = press(&mut world.app, KeyCode::Char('4'));
    assert_eq!(
        commands(&actions),
        [
            Command::ProjectSettings { project },
            Command::Plugin(muster_core::protocol::plugins::PluginCommand::List)
        ]
    );
    // Asked of a daemon that went away: asked again of the next.
    world.app.on_disconnected("gone".to_owned());
    world.app.on_connected("0.1.0".to_owned());
    let actions = press(&mut world.app, KeyCode::Down);
    assert_eq!(commands(&actions), [Command::ProjectSettings { project }]);
    let (req_id, _) = command(&actions);
    reply(
        &mut world,
        req_id,
        Ok(Reply::ProjectSettings(settings(
            false,
            SettingSource::Project,
        ))),
    );
    // Muster's own too, edited by hand: the agents the person added.
    let actions = press(&mut world.app, KeyCode::Char('r'));
    assert_eq!(
        commands(&actions),
        [
            Command::ReadPreferences,
            Command::ProjectSettings { project }
        ]
    );
}

#[test]
fn the_project_s_line_tells_where_its_settings_are_and_enter_there_does_nothing() {
    let mut world = on_settings(false);
    press(&mut world.app, KeyCode::Up);
    assert_eq!(
        world.app.settings_selected,
        Some(SettingRow::Project(world.project.id))
    );
    let screen = render(&world.app);
    assert!(
        screen.contains("Your settings /home/me/.config/muster/projects/p.toml"),
        "{screen}"
    );
    assert!(
        screen.contains("What you change here goes in yours."),
        "{screen}"
    );
    assert!(press(&mut world.app, KeyCode::Enter).is_empty());
    assert_eq!(world.app.mode, Mode::Browse);
}

#[test]
fn a_click_picks_a_setting_and_the_details_are_only_to_read() {
    let click = |column, row| MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    };
    let mut world = on_settings(false);
    let project = world.project.id;
    // The list's first line is below the top bar and its border.
    world.app.on_mouse(click(3, 2));
    assert_eq!(
        world.app.settings_selected,
        Some(SettingRow::Project(project))
    );
    world.app.on_mouse(click(3, 3));
    assert_eq!(
        world.app.settings_selected,
        Some(SettingRow::OnlyMine(project))
    );
    // A click on the details changes nothing and opens no dialog, there
    // or on the Tickets screen.
    let actions = world.app.on_mouse(click(60, 10));
    assert!(commands(&actions).is_empty());
    assert_eq!(world.app.mode, Mode::Browse);
    press(&mut world.app, KeyCode::Char('3'));
    world.app.on_mouse(click(60, 10));
    assert_eq!(world.app.mode, Mode::Browse);
    assert!(matches!(
        world.app.tickets.get(&project),
        Some(Tickets::Loading)
    ));
}

#[test]
fn branches_go_without_muster_once_turned_off() {
    let mut world = on_settings(false);
    let project = world.project.id;
    press(&mut world.app, KeyCode::Down);
    press(&mut world.app, KeyCode::Down);
    assert_eq!(
        world.app.settings_selected,
        Some(SettingRow::MusterPrefix(project))
    );
    let screen = render(&world.app);
    let row = screen
        .lines()
        .find(|line| line.contains("Branches with muster/"))
        .unwrap();
    assert!(
        row.trim_end_matches('│').trim_end().ends_with("on"),
        "{row}"
    );
    assert!(
        screen.contains("muster/issue-42-fix-the-login for a"),
        "{screen}"
    );
    assert!(screen.contains("⏎ turn off"), "{screen}");

    let actions = press(&mut world.app, KeyCode::Enter);
    let (req_id, change) = command(&actions);
    assert_eq!(
        change,
        Command::ChangeSetting {
            project,
            change: SettingChange::MusterPrefix(false)
        }
    );
    let mut off = settings(false, SettingSource::Default);
    off.muster_prefix = Some(Toggle {
        on: false,
        from: SettingSource::Yours,
    });
    let actions = reply(&mut world, req_id, Ok(Reply::ProjectSettings(off)));
    assert!(
        commands(&actions).is_empty(),
        "the tickets are not fetched again: {actions:?}"
    );
    assert_eq!(
        world.app.note.as_ref().unwrap().text,
        "New branches in app go without muster/: issue-42-fix-the-login."
    );
    let screen = render(&world.app);
    let row = screen
        .lines()
        .find(|line| line.contains("Branches with muster/"))
        .unwrap();
    assert!(row.contains("off") && !row.contains('…'), "{row}");
    assert!(screen.contains("worktree-4f2k9q for a"), "{screen}");
    assert!(screen.contains("⏎ or space turns it on"), "{screen}");
}

#[test]
fn shared_rust_builds_are_on_by_default_and_turned_off_on_the_settings_screen() {
    let mut world = world();
    press(&mut world.app, KeyCode::Char('4'));
    world.app.settings_selected = Some(SettingRow::SharedBuilds);
    let screen = render(&world.app);
    assert!(screen.contains("Shared Rust builds"), "{screen}");
    assert!(screen.contains("Cargo"), "what it writes is said: {screen}");
    let actions = press(&mut world.app, KeyCode::Enter);
    assert!(
        actions.iter().any(|action| matches!(
            action,
            Action::Send(ClientFrame::Command {
                command: Command::ChangePreference(
                    muster_core::settings::PreferenceChange::SharedBuilds(false)
                ),
                ..
            })
        )),
        "{actions:?}"
    );
}

#[test]
fn prd_01_agents_start_agents_until_turned_off_on_the_settings_screen() {
    let mut world = world();
    press(&mut world.app, KeyCode::Char('4'));
    world.app.settings_selected = Some(SettingRow::Delegation);
    let screen = render(&world.app);
    assert!(screen.contains("Agents start agents"), "{screen}");
    assert!(
        screen.contains("at most 4"),
        "its limits are said: {screen}"
    );
    let actions = press(&mut world.app, KeyCode::Enter);
    assert!(
        actions.iter().any(|action| matches!(
            action,
            Action::Send(ClientFrame::Command {
                command: Command::ChangePreference(
                    muster_core::settings::PreferenceChange::AgentsStartAgents(false)
                ),
                ..
            })
        )),
        "{actions:?}"
    );
}

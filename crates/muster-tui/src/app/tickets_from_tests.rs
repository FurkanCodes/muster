//! Choosing where a project's tickets come from on the Settings screen,
//! among the ticket plugins the daemon has (`PRD-15`).

use crossterm::event::KeyCode;
use muster_core::plugins::{Manifest, PluginSource, own};
use muster_core::protocol::plugins::{PluginDetails, PluginInfo, PluginProblem};
use muster_core::protocol::{Command, Delta, Reply, ServerFrame};
use muster_core::settings::{ProjectSettings, SettingChange, SettingSource, TicketsFrom};

use super::settings_tests::{on_settings, reply, settings};
use super::tests::{World, command, press, render, typed};
use super::{Dialog, Mode, SettingRow, Settings, Tickets};
use crate::model::tests::project;

/// A project's settings where nothing says where its tickets are and its
/// remote, on a company's own server, cannot tell.
fn untold() -> ProjectSettings {
    ProjectSettings {
        tickets_from: "10.10.10.5 is not a host Muster knows by its name; choose where its \
                       tickets are on the Settings screen"
            .to_owned(),
        tickets_untold: true,
        ..settings(true, SettingSource::Yours)
    }
}

fn from(tracker: &str, fields: &[(&str, &str)]) -> TicketsFrom {
    TicketsFrom {
        tracker: tracker.to_owned(),
        fields: fields
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect(),
    }
}

fn jira(site: &str, key: &str) -> TicketsFrom {
    from("jira", &[("site", site), ("project", key)])
}

/// A plugin as the daemon lists it, its manifest `text`.
fn plugin(text: &str, source: PluginSource) -> PluginInfo {
    let manifest = if source == PluginSource::Builtin {
        Manifest::parse_own(text)
    } else {
        Manifest::parse(text)
    }
    .unwrap();
    PluginInfo {
        id: manifest.id.clone(),
        source,
        enabled: true,
        problem: None,
        details: Some(PluginDetails::of(&manifest, "ab".repeat(32), |_| true)),
        changed: None,
    }
}

/// Anyone's ticket plugin, with a field it needs.
const YOUTRACK: &str = "id = \"youtrack\"\nname = \"YouTrack tickets\"\nversion = \"0.1.0\"\n\
                        muster = \"0.3\"\n\n[tickets]\nname = \"YouTrack\"\nnoun = \"issue\"\n\
                        command = [\"./youtrack\"]\n[[tickets.settings]]\nkey = \"board\"\n\
                        label = \"Board, such as APP\"\nrequired = true\n";

/// The daemon's plugins: Muster's own, and `theirs`.
fn with_plugins(world: &mut World, theirs: Vec<PluginInfo>) {
    let mut plugins: Vec<PluginInfo> = own::MANIFESTS
        .iter()
        .map(|text| plugin(text, PluginSource::Builtin))
        .collect();
    plugins.extend(theirs);
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::PluginsChanged(plugins)));
}

fn youtrack() -> PluginInfo {
    plugin(
        YOUTRACK,
        PluginSource::Folder {
            path: "/Users/me/youtrack".into(),
        },
    )
}

/// The world on the Settings screen with `settings` read, Tickets from
/// selected, and Muster's own ticket plugins and one of anyone's.
fn on_tickets_from(settings: ProjectSettings) -> World {
    let mut world = on_settings(true);
    with_plugins(&mut world, vec![youtrack()]);
    let project = world.project.id;
    world
        .app
        .settings
        .insert(project, Settings::Loaded(settings));
    press(&mut world.app, KeyCode::Down);
    assert_eq!(
        world.app.settings_selected,
        Some(SettingRow::TicketsFrom(project))
    );
    world
}

/// The dialog's tracker, each field's text, and which has the cursor.
fn dialog(world: &World) -> (Option<String>, Vec<String>, usize) {
    match &world.app.mode {
        Mode::Dialog(Dialog::TicketsFrom {
            tracker,
            fields,
            at,
            ..
        }) => (
            tracker.clone(),
            fields
                .iter()
                .map(|field| field.input.text().to_owned())
                .collect(),
            *at,
        ),
        other => panic!("no Tickets from dialog: {other:?}"),
    }
}

fn tracker(world: &World) -> Option<String> {
    dialog(world).0
}

fn change_asked(actions: &[super::Action]) -> (u64, SettingChange) {
    match command(actions) {
        (req_id, Command::ChangeSetting { change, .. }) => (req_id, change),
        (_, other) => panic!("not a setting change: {other:?}"),
    }
}

#[test]
fn a_project_whose_remote_cannot_tell_is_pointed_at_jira_here() {
    let mut world = on_tickets_from(untold());
    let screen = render(&world.app);
    assert!(screen.contains("Tickets from"), "{screen}");
    assert!(screen.contains("choose"), "{screen}");
    assert!(screen.contains("10.10.10.5 is not a host"), "{screen}");
    assert!(screen.contains("⏎ choose"), "{screen}");

    press(&mut world.app, KeyCode::Enter);
    assert_eq!(tracker(&world), None, "its git remote, to begin with");
    let screen = render(&world.app);
    for name in ["GitHub", "GitLab", "Jira", "Linear", "YouTrack"] {
        assert!(screen.contains(&format!(" {name} ")), "{name}: {screen}");
    }
    // With nothing to type, it still says how to choose.
    assert!(
        screen.contains("⇥ ⇧⇥ choose   ⏎ save   esc cancel"),
        "{screen}"
    );
    for _ in 0..3 {
        press(&mut world.app, KeyCode::Tab);
    }
    assert_eq!(tracker(&world).as_deref(), Some("jira"));
    let screen = render(&world.app);
    assert!(screen.contains("Jira site, such as"), "{screen}");
    assert!(
        screen.contains("Or a JQL query of your own (if you like)"),
        "{screen}"
    );

    // Not without its site: said, and the dialog stays.
    assert!(press(&mut world.app, KeyCode::Enter).is_empty());
    assert!(render(&world.app).contains("Jira needs its Jira site"));
    typed(&mut world.app, "acme.atlassian.net");
    press(&mut world.app, KeyCode::Down);
    assert_eq!(dialog(&world).2, 1, "↓ to the project's key");
    typed(&mut world.app, "app");

    let actions = press(&mut world.app, KeyCode::Enter);
    assert_eq!(world.app.mode, Mode::Browse);
    let (req_id, change) = change_asked(&actions);
    assert_eq!(
        change,
        SettingChange::TicketsFrom(Some(jira("acme.atlassian.net", "app")))
    );
    let project = world.project.id;
    world
        .app
        .tickets
        .insert(project, Tickets::Failed("x".to_owned()));
    reply(
        &mut world,
        req_id,
        Ok(Reply::ProjectSettings(ProjectSettings {
            tickets_from: "Jira at acme.atlassian.net, project APP".to_owned(),
            tickets: Some(jira("acme.atlassian.net", "APP")),
            tickets_set_by: SettingSource::Yours,
            ..settings(true, SettingSource::Yours)
        })),
    );
    let screen = render(&world.app);
    assert!(
        screen.contains("app's tickets now come from Jira at acme.atlassian.net, project APP."),
        "{screen}"
    );
    assert!(screen.contains("Jira APP"), "{screen}");
    assert!(
        !world.app.tickets.contains_key(&project),
        "fetched again from there"
    );
}

#[test]
fn the_dialog_starts_from_what_is_written_and_can_go_back_to_the_remote() {
    let mut written = jira("acme.atlassian.net", "APP");
    written
        .fields
        .insert("jql".to_owned(), "labels = ui".to_owned());
    let mut world = on_tickets_from(ProjectSettings {
        tickets: Some(written.clone()),
        ..settings(false, SettingSource::Default)
    });
    press(&mut world.app, KeyCode::Enter);
    assert_eq!(
        dialog(&world),
        (
            Some("jira".to_owned()),
            vec![
                "acme.atlassian.net".to_owned(),
                "APP".to_owned(),
                "labels = ui".to_owned()
            ],
            0
        )
    );
    // Saved as it was, a query of the person's own keeps.
    let (_, change) = change_asked(&press(&mut world.app, KeyCode::Enter));
    assert_eq!(change, SettingChange::TicketsFrom(Some(written.clone())));

    // ⇧⇥ from Jira is GitLab, which needs nothing typed.
    let project = world.project.id;
    world.app.settings.insert(
        project,
        Settings::Loaded(ProjectSettings {
            tickets: Some(written),
            ..settings(false, SettingSource::Default)
        }),
    );
    press(&mut world.app, KeyCode::Enter);
    press(&mut world.app, KeyCode::BackTab);
    assert_eq!(tracker(&world).as_deref(), Some("gitlab"));
    assert!(render(&world.app).contains("through your own glab login"));
    let (_, change) = change_asked(&press(&mut world.app, KeyCode::Enter));
    assert_eq!(
        change,
        SettingChange::TicketsFrom(Some(from("gitlab", &[])))
    );

    // Round to its git remote: nothing written any more.
    world.app.settings.insert(
        project,
        Settings::Loaded(settings(false, SettingSource::Default)),
    );
    press(&mut world.app, KeyCode::Enter);
    assert_eq!(tracker(&world), None);
    let (_, change) = change_asked(&press(&mut world.app, KeyCode::Enter));
    assert_eq!(change, SettingChange::TicketsFrom(None));
}

#[test]
fn anyone_s_ticket_plugin_is_offered_with_the_fields_it_declares() {
    let mut world = on_tickets_from(untold());
    press(&mut world.app, KeyCode::Enter);
    // Last of all, after Muster's own.
    press(&mut world.app, KeyCode::BackTab);
    assert_eq!(tracker(&world).as_deref(), Some("youtrack"));
    let screen = render(&world.app);
    assert!(screen.contains("Board, such as APP:"), "{screen}");
    press(&mut world.app, KeyCode::Enter);
    assert!(render(&world.app).contains("YouTrack needs its Board, such as APP."));
    world.app.on_paste("WEB");
    let (req_id, change) = change_asked(&press(&mut world.app, KeyCode::Enter));
    assert_eq!(
        change,
        SettingChange::TicketsFrom(Some(from("youtrack", &[("board", "WEB")])))
    );
    reply(
        &mut world,
        req_id,
        Ok(Reply::ProjectSettings(ProjectSettings {
            tickets_from: "YouTrack tickets, board WEB".to_owned(),
            tickets: Some(from("youtrack", &[("board", "WEB")])),
            tickets_set_by: SettingSource::Yours,
            ..settings(true, SettingSource::Yours)
        })),
    );
    assert!(render(&world.app).contains("YouTrack WEB"));

    // Turned off, or unable to run, it is offered no more.
    let mut off = youtrack();
    off.enabled = false;
    let mut changed = youtrack();
    changed.problem = Some(PluginProblem::Changed);
    for gone in [off, changed] {
        with_plugins(&mut world, vec![gone]);
        assert!(
            world
                .app
                .offered_trackers()
                .iter()
                .all(|offered| offered.id != "youtrack")
        );
    }
}

#[test]
fn what_was_typed_for_a_tracker_waits_as_tab_goes_round() {
    let mut world = on_tickets_from(untold());
    press(&mut world.app, KeyCode::Enter);
    press(&mut world.app, KeyCode::BackTab);
    press(&mut world.app, KeyCode::BackTab);
    assert_eq!(tracker(&world).as_deref(), Some("linear"));
    world.app.on_paste("eng");
    press(&mut world.app, KeyCode::Tab);
    press(&mut world.app, KeyCode::BackTab);
    assert_eq!(dialog(&world).1, ["eng"]);
    let (_, change) = change_asked(&press(&mut world.app, KeyCode::Enter));
    assert_eq!(
        change,
        SettingChange::TicketsFrom(Some(from("linear", &[("team", "eng")])))
    );
}

#[test]
fn a_jira_site_in_use_is_offered_to_the_next_project() {
    let mut world = on_tickets_from(untold());
    let other = project("internal");
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::ProjectUpserted(other.clone())));
    world.app.settings.insert(
        other.id,
        Settings::Loaded(ProjectSettings {
            tickets: Some(jira("acme.atlassian.net", "INT")),
            ..settings(true, SettingSource::Yours)
        }),
    );
    press(&mut world.app, KeyCode::Enter);
    for _ in 0..3 {
        press(&mut world.app, KeyCode::Tab);
    }
    let (_, fields, at) = dialog(&world);
    assert_eq!(fields, ["acme.atlassian.net", "", ""]);
    assert_eq!(at, 1, "the site is in: the project's key is next");
    // ↑ back to the site; the paste goes there.
    press(&mut world.app, KeyCode::Up);
    world.app.on_paste(".example");
    assert_eq!(dialog(&world).1[0], "acme.atlassian.net.example");
    // Esc leaves it as it was.
    press(&mut world.app, KeyCode::Esc);
    assert_eq!(world.app.mode, Mode::Browse);
}

#[test]
fn turning_a_ticket_plugin_off_or_on_fetches_the_tickets_again_prd_15() {
    let mut world = super::tickets_tests::on_tickets();
    let project = world.project.id;
    let listed = Command::ListIssues { project };
    let mut github: Vec<PluginInfo> = own::MANIFESTS
        .iter()
        .map(|text| plugin(text, PluginSource::Builtin))
        .collect();
    let send = |world: &mut World, plugins: &[PluginInfo]| {
        world
            .app
            .on_frame(ServerFrame::Delta(Delta::PluginsChanged(plugins.to_vec())))
    };
    let sent = |world: &mut World, plugins: &[PluginInfo]| {
        super::tickets_tests::commands(&send(world, plugins))
    };
    // Muster's own plugins, as the daemon first tells of them.
    let (req_id, asked) = command(&send(&mut world, &github));
    assert_eq!(asked, listed);
    world.app.on_frame(ServerFrame::Reply {
        req_id,
        outcome: Ok(Reply::Issues {
            source: None,
            tracker: "GitHub issues".to_owned(),
            issues: Vec::new(),
            mine: false,
        }),
    });
    // The same plugins told again keep the tickets fetched.
    assert!(sent(&mut world, &github).is_empty());
    assert!(matches!(
        world.app.tickets.get(&project),
        Some(Tickets::Loaded { .. })
    ));

    // GitHub issues turned off: the tickets shown came from it, so they
    // are fetched again, and the daemon says why there are none.
    for plugin in github.iter_mut().filter(|plugin| plugin.id == "github") {
        plugin.enabled = false;
    }
    assert_eq!(sent(&mut world, &github), std::slice::from_ref(&listed));
    assert!(world.app.ticket_texts.is_empty());
    // Turned on again, they are fetched once more.
    for plugin in github.iter_mut().filter(|plugin| plugin.id == "github") {
        plugin.enabled = true;
    }
    world.app.tickets.clear();
    world
        .app
        .tickets
        .insert(project, Tickets::Failed("off".to_owned()));
    assert_eq!(sent(&mut world, &github), std::slice::from_ref(&listed));
    // A plugin with no ticket source changing leaves them be.
    let mut panel = youtrack();
    panel.details.as_mut().unwrap().tickets = None;
    github.push(panel);
    world
        .app
        .tickets
        .insert(project, Tickets::Failed("kept".to_owned()));
    assert!(sent(&mut world, &github).is_empty());
    // Anyone's ticket plugin installed changes where tickets may come from.
    github.push(youtrack());
    assert_eq!(sent(&mut world, &github), [listed]);
}

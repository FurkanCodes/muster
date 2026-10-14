//! Choosing where a project's tickets come from on the Settings screen.

use crossterm::event::KeyCode;
use muster_core::config::TrackerKind;
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

fn jira(site: &str, key: &str) -> TicketsFrom {
    TicketsFrom {
        tracker: TrackerKind::Jira,
        site: Some(site.to_owned()),
        project: Some(key.to_owned()),
        jql: None,
        team: None,
    }
}

/// The world on the Settings screen with `settings` read, Tickets from
/// selected.
fn on_tickets_from(settings: ProjectSettings) -> World {
    let mut world = on_settings(true);
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

fn dialog(world: &World) -> (Option<TrackerKind>, String, String, bool) {
    match &world.app.mode {
        Mode::Dialog(Dialog::TicketsFrom {
            tracker,
            site,
            key,
            on_key,
            ..
        }) => (
            *tracker,
            site.text().to_owned(),
            key.text().to_owned(),
            *on_key,
        ),
        other => panic!("no Tickets from dialog: {other:?}"),
    }
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
    assert_eq!(dialog(&world).0, None, "its git remote, to begin with");
    for _ in 0..3 {
        press(&mut world.app, KeyCode::Tab);
    }
    assert_eq!(dialog(&world).0, Some(TrackerKind::Jira));
    assert!(render(&world.app).contains("Jira site, such as"));

    // Not without its site: said, and the dialog stays.
    assert!(press(&mut world.app, KeyCode::Enter).is_empty());
    assert!(render(&world.app).contains("Jira needs its site"));
    typed(&mut world.app, "acme.atlassian.net");
    // Nor without its project.
    press(&mut world.app, KeyCode::Enter);
    assert!(render(&world.app).contains("Jira needs the project's key"));
    assert!(dialog(&world).3, "the cursor goes to what is missing");
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
    let written = TicketsFrom {
        jql: Some("labels = ui".to_owned()),
        ..jira("acme.atlassian.net", "APP")
    };
    let mut world = on_tickets_from(ProjectSettings {
        tickets: Some(written.clone()),
        ..settings(false, SettingSource::Default)
    });
    press(&mut world.app, KeyCode::Enter);
    assert_eq!(
        dialog(&world),
        (
            Some(TrackerKind::Jira),
            "acme.atlassian.net".to_owned(),
            "APP".to_owned(),
            true
        )
    );
    // Saved as it was, a query written by hand keeps.
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
    assert_eq!(dialog(&world).0, Some(TrackerKind::GitLab));
    assert!(render(&world.app).contains("through your glab login"));
    let (_, change) = change_asked(&press(&mut world.app, KeyCode::Enter));
    assert_eq!(
        change,
        SettingChange::TicketsFrom(Some(TicketsFrom {
            tracker: TrackerKind::GitLab,
            site: None,
            project: None,
            jql: None,
            team: None,
        }))
    );

    // Round to its git remote: nothing written any more.
    world.app.settings.insert(
        project,
        Settings::Loaded(settings(false, SettingSource::Default)),
    );
    press(&mut world.app, KeyCode::Enter);
    assert_eq!(dialog(&world).0, None);
    let (_, change) = change_asked(&press(&mut world.app, KeyCode::Enter));
    assert_eq!(change, SettingChange::TicketsFrom(None));
}

#[test]
fn linear_needs_a_team_and_a_paste_goes_where_the_cursor_is() {
    let mut world = on_tickets_from(untold());
    press(&mut world.app, KeyCode::Enter);
    press(&mut world.app, KeyCode::BackTab);
    assert_eq!(dialog(&world).0, Some(TrackerKind::Linear));
    press(&mut world.app, KeyCode::Enter);
    assert!(render(&world.app).contains("Linear needs the team's key"));
    world.app.on_paste("eng");
    let (_, change) = change_asked(&press(&mut world.app, KeyCode::Enter));
    assert_eq!(
        change,
        SettingChange::TicketsFrom(Some(TicketsFrom {
            tracker: TrackerKind::Linear,
            site: None,
            project: None,
            jql: None,
            team: Some("eng".to_owned()),
        }))
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
    let (_, site, key, on_key) = dialog(&world);
    assert_eq!((site.as_str(), key.as_str()), ("acme.atlassian.net", ""));
    assert!(on_key, "the site is in: the project's key is next");
    // ↑ back to the site; the paste goes there.
    press(&mut world.app, KeyCode::Up);
    world.app.on_paste(".example");
    assert_eq!(dialog(&world).1, "acme.atlassian.net.example");
    // Esc leaves it as it was.
    press(&mut world.app, KeyCode::Esc);
    assert_eq!(world.app.mode, Mode::Browse);
}

#[test]
fn an_older_daemon_is_never_sent_the_change() {
    let mut world = on_tickets_from(untold());
    world.app.daemon_protocol = 16;
    assert!(press(&mut world.app, KeyCode::Enter).is_empty());
    assert_eq!(world.app.mode, Mode::Browse);
    assert!(render(&world.app).contains("muster reload"));
}

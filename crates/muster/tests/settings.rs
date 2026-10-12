//! End-to-end tests for what the Settings screen asks of the daemon:
//! reading a project's settings, and turning only-my-tickets on and off.
//! A change goes into the person's own overrides, never into the
//! repository (`ARC-05`), and changes what the tracker is asked for; a
//! stand-in `gh` records how it was asked.

// Helpers outside `#[test]` functions are not covered by clippy.toml's test exemption.
#![allow(clippy::unwrap_used)]

mod support;

use muster_client::{ClientError, Connection};
use muster_core::ProjectId;
use muster_core::protocol::{Command, ErrorBody, ErrorCode, Reply};
use muster_core::settings::{ProjectSettings, SettingChange, SettingSource};

use support::{Sandbox, git};

async fn add(connection: &mut Connection, root: std::path::PathBuf) -> ProjectId {
    let Reply::ProjectAdded { id } = connection
        .command(Command::AddProject { root, name: None })
        .await
        .unwrap()
    else {
        panic!("expected ProjectAdded");
    };
    id
}

async fn settings(connection: &mut Connection, command: Command) -> ProjectSettings {
    match connection.command(command).await.unwrap() {
        Reply::ProjectSettings(settings) => settings,
        other => panic!("expected ProjectSettings, got {other:?}"),
    }
}

async fn refused(connection: &mut Connection, command: Command) -> ErrorBody {
    match connection.command(command).await {
        Err(ClientError::Daemon(error)) => error,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

/// Whether the latest listing asked `gh` for only the person's issues.
async fn listed_only_mine(
    connection: &mut Connection,
    sandbox: &Sandbox,
    project: ProjectId,
) -> bool {
    let Reply::Issues { mine, .. } = connection
        .command(Command::ListIssues { project })
        .await
        .unwrap()
    else {
        panic!("expected Issues");
    };
    let calls = std::fs::read_to_string(sandbox.fake_gh().join("calls")).unwrap();
    let last = calls.lines().last().unwrap().to_owned();
    assert_eq!(last.ends_with("--assignee @me"), mine, "{last}");
    mine
}

#[tokio::test]
async fn only_my_tickets_goes_into_the_person_s_settings_and_narrows_the_listing() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    let repository_settings = "[checks]\ncommands = [\"true\"]\n";
    std::fs::write(root.join("muster.toml"), repository_settings).unwrap();
    std::fs::write(sandbox.fake_gh().join("issues.json"), "[]").unwrap();
    let daemon = sandbox.start();
    let mut connection = daemon.connect().await;
    let project = add(&mut connection, root.clone()).await;
    let file = sandbox.paths().project_overrides(project);

    let before = settings(&mut connection, Command::ProjectSettings { project }).await;
    assert_eq!(before.tickets_from, "GitHub issues, as its git remote says");
    assert!(!before.only_mine.on);
    assert_eq!(before.only_mine.from, SettingSource::Default);
    assert_eq!(before.file, file);
    assert!(!file.exists(), "reading writes nothing");
    assert!(!listed_only_mine(&mut connection, &sandbox, project).await);

    let on = settings(
        &mut connection,
        Command::ChangeSetting {
            project,
            change: SettingChange::OnlyMyTickets(true),
        },
    )
    .await;
    assert!(on.only_mine.on);
    assert_eq!(on.only_mine.from, SettingSource::Yours);
    let written = std::fs::read_to_string(&file).unwrap();
    assert!(written.ends_with("[tickets]\nmine = true\n"), "{written}");
    assert!(listed_only_mine(&mut connection, &sandbox, project).await);
    // The repository is only read: its file as it was, nothing new in it.
    assert_eq!(
        std::fs::read_to_string(root.join("muster.toml")).unwrap(),
        repository_settings
    );
    assert_eq!(
        git(&root, &["status", "--porcelain", "--untracked-files=all"]),
        "?? muster.toml"
    );
    let config = sandbox.run(&["project", "config", "app"]);
    let said = String::from_utf8_lossy(&config.stdout).into_owned();
    assert!(
        said.contains("tickets       as the git remote says, assigned to you"),
        "{said}"
    );

    let off = settings(
        &mut connection,
        Command::ChangeSetting {
            project,
            change: SettingChange::OnlyMyTickets(false),
        },
    )
    .await;
    assert!(!off.only_mine.on);
    assert_eq!(off.only_mine.from, SettingSource::Yours);
    assert!(!listed_only_mine(&mut connection, &sandbox, project).await);
}

#[tokio::test]
async fn the_person_s_setting_wins_over_the_repository_s() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    std::fs::write(root.join("muster.toml"), "[tickets]\nmine = true\n").unwrap();
    let daemon = sandbox.start();
    let mut connection = daemon.connect().await;
    let project = add(&mut connection, root).await;

    let read = settings(&mut connection, Command::ProjectSettings { project }).await;
    assert!(read.only_mine.on);
    assert_eq!(read.only_mine.from, SettingSource::Project);
    let off = settings(
        &mut connection,
        Command::ChangeSetting {
            project,
            change: SettingChange::OnlyMyTickets(false),
        },
    )
    .await;
    assert!(!off.only_mine.on);
    assert_eq!(off.only_mine.from, SettingSource::Yours);
}

#[tokio::test]
async fn settings_that_do_not_read_are_said_and_a_change_writes_nothing() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    let daemon = sandbox.start();
    let mut connection = daemon.connect().await;
    let project = add(&mut connection, root).await;
    let file = sandbox.paths().project_overrides(project);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, "[tickets]\nmien = true\n").unwrap();

    let error = refused(&mut connection, Command::ProjectSettings { project }).await;
    assert_eq!(error.code, ErrorCode::BadRequest);
    assert!(error.message.contains("mien"), "{}", error.message);
    let error = refused(
        &mut connection,
        Command::ChangeSetting {
            project,
            change: SettingChange::OnlyMyTickets(true),
        },
    )
    .await;
    assert_eq!(error.code, ErrorCode::BadRequest);
    assert!(error.message.contains("mien"), "{}", error.message);
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "[tickets]\nmien = true\n"
    );

    let unknown = ProjectId::new();
    let error = refused(
        &mut connection,
        Command::ProjectSettings { project: unknown },
    )
    .await;
    assert_eq!(error.code, ErrorCode::NotFound);
}

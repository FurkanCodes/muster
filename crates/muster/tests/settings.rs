//! End-to-end tests for what the Settings screen asks of the daemon:
//! reading a project's settings, and turning only-my-tickets on and off.
//! A change goes into the person's own overrides, never into the
//! repository (`ARC-05`), and changes what the tracker is asked for; a
//! stand-in `gh` records how it was asked.

// Helpers outside `#[test]` functions are not covered by clippy.toml's test exemption.
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "end-to-end tests: a failed step is the test failing"
)]
#![expect(
    clippy::disallowed_methods,
    reason = "tests time themselves and start their own threads and tasks; ARC-03, ARC-13 and ARC-14 bind Muster's own code"
)]

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

#[tokio::test]
async fn a_project_on_a_company_s_own_server_gets_its_tickets_from_jira_here() {
    use muster_core::config::TrackerKind;
    use muster_core::settings::TicketsFrom;

    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    // Its git server is on the company's network, by its address.
    git(
        &root,
        &["remote", "set-url", "github", "git@10.10.10.5:team/app.git"],
    );
    let daemon = sandbox.start();
    let mut connection = daemon.connect().await;
    let project = add(&mut connection, root.clone()).await;
    let file = sandbox.paths().project_overrides(project);

    let untold = settings(&mut connection, Command::ProjectSettings { project }).await;
    assert!(untold.tickets_untold);
    assert_eq!(untold.tickets, None);
    assert!(
        untold.tickets_from.contains("10.10.10.5 is not a host")
            && untold.tickets_from.contains("on the Settings screen"),
        "{}",
        untold.tickets_from
    );
    let listing = refused(&mut connection, Command::ListIssues { project }).await;
    assert!(
        listing.message.contains("on the Settings screen"),
        "{listing:?}"
    );

    let jira = TicketsFrom {
        tracker: TrackerKind::Jira,
        site: Some("acme.atlassian.net".to_owned()),
        project: Some("app".to_owned()),
        jql: None,
        team: None,
    };
    let chosen = settings(
        &mut connection,
        Command::ChangeSetting {
            project,
            change: SettingChange::TicketsFrom(Some(jira)),
        },
    )
    .await;
    assert!(!chosen.tickets_untold);
    assert_eq!(
        chosen.tickets_from,
        "Jira at acme.atlassian.net, project APP"
    );
    assert_eq!(chosen.tickets_set_by, SettingSource::Yours);
    assert_eq!(
        chosen
            .tickets
            .as_ref()
            .and_then(|from| from.project.as_deref()),
        Some("APP")
    );
    let written = std::fs::read_to_string(&file).unwrap();
    assert!(
        written.ends_with(
            "[tickets]\ntracker = \"jira\"\nsite = \"acme.atlassian.net\"\nproject = \"app\"\n"
        ),
        "{written}"
    );
    assert_eq!(
        git(&root, &["status", "--porcelain", "--untracked-files=all"]),
        "",
        "nothing in the repository"
    );

    // Jira without its site is refused, the file as it was.
    let error = refused(
        &mut connection,
        Command::ChangeSetting {
            project,
            change: SettingChange::TicketsFrom(Some(TicketsFrom {
                tracker: TrackerKind::Jira,
                site: None,
                project: Some("APP".to_owned()),
                jql: None,
                team: None,
            })),
        },
    )
    .await;
    assert_eq!(error.code, ErrorCode::BadRequest);
    assert!(error.message.contains("tickets.site"), "{error:?}");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), written);

    // Back to its git remote, which still cannot tell.
    let back = settings(
        &mut connection,
        Command::ChangeSetting {
            project,
            change: SettingChange::TicketsFrom(None),
        },
    )
    .await;
    assert!(back.tickets_untold);
    assert_eq!(back.tickets_set_by, SettingSource::Default);
}

#[tokio::test]
async fn branches_go_without_muster_once_the_person_says_so() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    std::fs::write(
        sandbox.fake_gh().join("issue-3.json"),
        serde_json::json!({"number": 3, "title": "Typo in README",
            "url": "https://github.com/o/app/issues/3", "labels": [],
            "author": {"login": "x"}, "body": "teh"})
        .to_string(),
    )
    .unwrap();
    let daemon = sandbox.start();
    let mut connection = daemon.connect().await;
    let project = add(&mut connection, root).await;
    let file = sandbox.paths().project_overrides(project);

    let before = settings(&mut connection, Command::ProjectSettings { project }).await;
    let prefix = before.muster_prefix.unwrap();
    assert!(prefix.on);
    assert_eq!(prefix.from, SettingSource::Default);
    let off = settings(
        &mut connection,
        Command::ChangeSetting {
            project,
            change: SettingChange::MusterPrefix(false),
        },
    )
    .await;
    let prefix = off.muster_prefix.unwrap();
    assert!(!prefix.on);
    assert_eq!(prefix.from, SettingSource::Yours);
    let written = std::fs::read_to_string(&file).unwrap();
    assert!(
        written.ends_with("[branches]\nmuster_prefix = false\n"),
        "{written}"
    );
    let config = sandbox.run(&["project", "config", "app"]);
    let said = String::from_utf8_lossy(&config.stdout).into_owned();
    assert!(
        said.contains("branches      issue-42-…, without muster/"),
        "{said}"
    );

    // A ticket's branch, and a worktree's with no name given.
    let Reply::IssueStarted { workspace, .. } = connection
        .command(Command::StartFromIssue {
            project,
            key: muster_core::TicketKey::number(3),
            harness: "shell".to_owned(),
            size: muster_core::protocol::TermSize::default(),
            another: false,
        })
        .await
        .unwrap()
    else {
        panic!("expected IssueStarted");
    };
    let Reply::WorktreeCreated { id: unnamed } = connection
        .command(Command::CreateWorktree {
            project,
            branch: None,
        })
        .await
        .unwrap()
    else {
        panic!("expected WorktreeCreated");
    };
    let snapshot = connection.subscribe().await.unwrap();
    let branch = |id| {
        snapshot
            .workspaces
            .iter()
            .find(|found| found.id == id)
            .unwrap()
            .kind
            .branch()
            .to_owned()
    };
    assert_eq!(branch(workspace), "issue-3-typo-in-readme");
    let unnamed = branch(unnamed);
    assert!(
        unnamed.starts_with("worktree-") && unnamed.len() == "worktree-".len() + 6,
        "{unnamed}"
    );
    // Listed as any worktree is.
    let status = String::from_utf8_lossy(&sandbox.run(&["status"]).stdout).into_owned();
    assert!(status.contains("  issue-3-typo-in-readme  "), "{status}");
}

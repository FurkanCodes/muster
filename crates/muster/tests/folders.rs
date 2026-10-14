//! End-to-end tests for projects that are plain folders, not git
//! repositories: sessions and checks run in the folder, what needs git is
//! refused plainly, and it all becomes possible once the folder is made a
//! repository.

// Helpers outside `#[test]` functions are not covered by clippy.toml's test exemption.
#![allow(clippy::unwrap_used)]

mod support;

use std::process::Output;

use muster_core::protocol::{Command, Delta, Reply, ServerFrame, TermSize};

use support::{Sandbox, expect_event, git, read_terminal};

fn said(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[tokio::test]
async fn a_plain_folder_is_a_project_until_and_after_it_becomes_a_repository() {
    let sandbox = Sandbox::new().with_env("MUSTER_BRANCH_MS", "50");
    let folder = sandbox.root().join("Desktop").join("notes");
    std::fs::create_dir_all(&folder).unwrap();
    let folder = folder.canonicalize().unwrap();
    let daemon = sandbox.start();

    let added = sandbox.run(&["project", "add", folder.to_str().unwrap()]);
    assert!(added.status.success(), "{}", said(&added));
    let mut connection = daemon.connect().await;
    let snapshot = connection.subscribe().await.unwrap();
    let project = snapshot.projects[0].id;
    let own = snapshot.workspaces[0].clone();
    assert_eq!(own.project, project);
    assert!(own.kind.is_checkout() && !own.kind.in_git(), "{own:?}");
    let status = sandbox.run(&["status"]);
    assert!(
        said(&status).contains("(not a git repository)"),
        "{}",
        said(&status)
    );

    // A session runs in the folder.
    let Reply::SessionStarted { id: session } = connection
        .command(Command::StartSession {
            workspace: own.id,
            harness: "shell".to_owned(),
            title: None,
            size: TermSize::default(),
            prompt: None,
        })
        .await
        .unwrap()
    else {
        panic!("expected SessionStarted");
    };
    connection.attach(session, None).await.unwrap();
    connection
        .input(session, b"pwd; echo here-$((3*4))\n")
        .await
        .unwrap();
    let (text, _) = read_terminal(&mut connection, session, "here-12").await;
    assert!(text.contains(folder.to_str().unwrap()), "{text}");

    // So do its checks.
    std::fs::write(
        folder.join("muster.toml"),
        "[checks]\ncommands = [\"echo checked-$((6*7))\"]\n",
    )
    .unwrap();
    let checked = sandbox.run(&["check", "notes"]);
    assert!(checked.status.success(), "{}", said(&checked));
    assert!(said(&checked).contains("checked-42"), "{}", said(&checked));

    // What needs git is refused, saying why; tickets, which can be in Jira
    // or Linear for a folder too, say how to name where they are.
    for (args, why) in [
        (
            &["worktree", "add", "notes", "--branch", "side"][..],
            "worktrees need one",
        ),
        (
            &["issue", "list", "notes"][..],
            "choose where its tickets are on the Settings screen, or say with [tickets] tracker",
        ),
        (&["review", "notes"][..], "no branch to review or push"),
        (&["pr", "notes"][..], "no branch to review or push"),
    ] {
        let refused = sandbox.run(args);
        assert!(!refused.status.success(), "{args:?}: {}", said(&refused));
        assert!(said(&refused).contains(why), "{args:?}: {}", said(&refused));
    }
    assert!(!folder.join(".git").exists(), "Muster made no repository");

    // It is still a folder after a restart.
    drop(connection);
    assert!(daemon.stop().success());
    let daemon = sandbox.start();
    let mut connection = daemon.connect().await;
    let snapshot = connection.subscribe().await.unwrap();
    assert_eq!(snapshot.projects[0].id, project);
    assert_eq!(snapshot.workspaces.len(), 1);
    assert!(!snapshot.workspaces[0].kind.in_git());

    // Made a repository, it shows its branch and has worktrees.
    git(&folder, &["init", "--quiet", "--initial-branch", "main"]);
    git(&folder, &["add", "muster.toml"]);
    git(&folder, &["commit", "--quiet", "--message", "first"]);
    expect_event(&mut connection, |frame| match frame {
        ServerFrame::Delta(Delta::WorkspaceUpserted(found))
            if found.id == own.id && found.kind.branch() == "main" =>
        {
            Some(())
        }
        _ => None,
    })
    .await;
    let made = sandbox.run(&["worktree", "add", "notes", "--branch", "side"]);
    assert!(made.status.success(), "{}", said(&made));
    assert_eq!(
        git(
            &folder,
            &["branch", "--list", "--format=%(refname:short)", "side"]
        ),
        "side"
    );
}

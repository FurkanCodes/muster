//! The folder picker in the Add-project dialog: folders listed as the
//! person types, chosen with the arrows, opened with ⇥, added with ⏎.

use std::path::PathBuf;

use crossterm::event::KeyCode;
use muster_core::protocol::Command;

use super::tests::{World, command, press, render, typed, world};
use super::{Action, App, Dialog, FolderPicker, Mode};
use crate::folders::Folder;

fn folder(name: &str, git: bool) -> Folder {
    Folder {
        name: name.to_owned(),
        git,
    }
}

/// The folders in `/src`, where the world's `muster` was started.
fn src() -> Vec<Folder> {
    vec![
        folder(".config", false),
        folder("app", true),
        folder("Desktop", false),
        folder("muster", true),
        folder("notes", false),
        folder("old-app", false),
    ]
}

fn picker(app: &App) -> (&str, &FolderPicker) {
    match &app.mode {
        Mode::Dialog(Dialog::AddProject { path, picker, .. }) => (path.text(), picker),
        other => panic!("expected the add-project dialog, got {other:?}"),
    }
}

/// The world with the dialog open and `/src` listed.
fn opened() -> World {
    let mut world = world();
    let actions = press(&mut world.app, KeyCode::Char('a'));
    assert_eq!(actions, [Action::ListFolders(PathBuf::from("/src"))]);
    world.app.on_folders(&PathBuf::from("/src"), Ok(src()));
    world
}

fn added(actions: &[Action]) -> PathBuf {
    match command(actions).1 {
        Command::AddProject { root, .. } => root,
        other => panic!("expected AddProject, got {other:?}"),
    }
}

#[test]
fn opening_lists_where_muster_was_started_and_enter_adds_it() {
    let mut world = opened();
    let screen = render(&world.app);
    for shown in [
        "app/  git",
        "Desktop/",
        "muster/  git",
        "notes/",
        "old-app/",
    ] {
        assert!(screen.contains(shown), "{shown:?} in {screen}");
    }
    assert!(!screen.contains(".config"), "hidden: {screen}");
    // Nothing chosen: the typed folder itself.
    assert_eq!(picker(&world.app).1.selected, None);
    assert_eq!(
        added(&press(&mut world.app, KeyCode::Enter)),
        PathBuf::from("/src/")
    );
}

#[test]
fn typing_narrows_the_list_and_chooses_the_first_match() {
    let mut world = opened();
    typed(&mut world.app, "APP");
    let screen = render(&world.app);
    assert!(screen.contains("› app/"), "{screen}");
    assert!(screen.contains("old-app/"), "{screen}");
    assert!(!screen.contains("notes/"), "{screen}");
    // Typing a name does not list again: the folder is the same.
    assert!(
        !press(&mut world.app, KeyCode::Down).contains(&Action::ListFolders(PathBuf::from("/src")))
    );
    assert_eq!(
        added(&press(&mut world.app, KeyCode::Enter)),
        PathBuf::from("/src/old-app")
    );

    let mut world = opened();
    typed(&mut world.app, "nothing-like-it");
    assert!(render(&world.app).contains("No folder here matches"));
    assert_eq!(
        added(&press(&mut world.app, KeyCode::Enter)),
        PathBuf::from("/src/nothing-like-it"),
        "the typed path, for the daemon to judge"
    );

    let mut world = opened();
    typed(&mut world.app, ".");
    assert!(
        render(&world.app).contains(".config/"),
        "hidden ones once a dot is typed"
    );
}

#[test]
fn arrows_choose_and_up_from_the_first_is_the_typed_folder() {
    let mut world = opened();
    press(&mut world.app, KeyCode::Down);
    press(&mut world.app, KeyCode::Down);
    assert!(render(&world.app).contains("› Desktop/"));
    press(&mut world.app, KeyCode::Up);
    press(&mut world.app, KeyCode::Up);
    assert_eq!(picker(&world.app).1.selected, None);
    for _ in 0..20 {
        press(&mut world.app, KeyCode::Down);
    }
    assert_eq!(picker(&world.app).1.selected, Some(4), "stops at the last");
    assert_eq!(
        added(&press(&mut world.app, KeyCode::Enter)),
        PathBuf::from("/src/old-app")
    );
}

#[test]
fn tab_opens_a_folder_and_shift_tab_goes_back_up() {
    let mut world = opened();
    typed(&mut world.app, "mus");
    let actions = press(&mut world.app, KeyCode::Tab);
    assert_eq!(picker(&world.app).0, "/src/muster/");
    assert_eq!(actions, [Action::ListFolders(PathBuf::from("/src/muster"))]);
    assert!(render(&world.app).contains("Loading folder"));

    // A listing for a folder no longer shown is dropped.
    world.app.on_folders(&PathBuf::from("/src"), Ok(src()));
    assert!(render(&world.app).contains("Loading folder"));
    world.app.on_folders(
        &PathBuf::from("/src/muster"),
        Ok(vec![folder("crates", false)]),
    );
    assert!(render(&world.app).contains("crates/"));

    let actions = press(&mut world.app, KeyCode::BackTab);
    assert_eq!(picker(&world.app).0, "/src/");
    assert_eq!(actions, [Action::ListFolders(PathBuf::from("/src"))]);
    press(&mut world.app, KeyCode::BackTab);
    press(&mut world.app, KeyCode::BackTab);
    assert_eq!(picker(&world.app).0, "/", "the top is as far as it goes");
}

#[test]
fn a_folder_that_cannot_be_read_says_why_and_typing_still_works() {
    let mut world = world();
    press(&mut world.app, KeyCode::Char('a'));
    world.app.on_key(super::tests::ctrl('u'));
    typed(&mut world.app, "/private/");
    world.app.on_folders(
        &PathBuf::from("/private"),
        Err("cannot read it: Permission denied (os error 13)".to_owned()),
    );
    assert!(render(&world.app).contains("Permission denied"));
    typed(&mut world.app, "x");
    assert_eq!(
        added(&press(&mut world.app, KeyCode::Enter)),
        PathBuf::from("/private/x")
    );
}

#[test]
fn a_long_list_scrolls_with_the_choice_and_says_where_it_is() {
    let mut world = world();
    press(&mut world.app, KeyCode::Char('a'));
    let many = (1..=20)
        .map(|n| folder(&format!("f{n:02}"), false))
        .collect();
    world.app.on_folders(&PathBuf::from("/src"), Ok(many));
    assert!(render(&world.app).contains("1–8 of 20"));
    for _ in 0..10 {
        press(&mut world.app, KeyCode::Down);
    }
    let screen = render(&world.app);
    assert!(screen.contains("› f10/"), "{screen}");
    assert!(screen.contains("3–10 of 20"), "{screen}");
    assert!(!screen.contains("f02/"), "{screen}");
    press(&mut world.app, KeyCode::PageDown);
    assert!(render(&world.app).contains("› f18/"));
}

#[test]
fn pasting_a_path_lists_its_folder() {
    let mut world = world();
    press(&mut world.app, KeyCode::Char('a'));
    world.app.on_key(super::tests::ctrl('u'));
    let actions = world.app.on_paste("/home/me/Desktop/");
    assert_eq!(
        actions,
        [Action::ListFolders(PathBuf::from("/home/me/Desktop"))]
    );
}

#[test]
fn g_clones_a_github_repository_into_the_folder_chosen() {
    use muster_core::protocol::repositories::HostedRepository;
    use muster_core::protocol::{Reply, ServerFrame};

    let mut world = world();
    let actions = press(&mut world.app, KeyCode::Char('g'));
    let (req_id, asked) = command(&actions);
    assert_eq!(asked, Command::ListRepositories);
    assert!(render(&world.app).contains("Loading repositories"));
    let repository = |name: &str, private: bool| HostedRepository {
        name: name.to_owned(),
        description: Some(format!("{name}, described")),
        private,
    };
    world.app.on_frame(ServerFrame::Reply {
        req_id,
        outcome: Ok(Reply::Repositories(vec![
            repository("o/site", false),
            repository("o/app", true),
        ])),
    });
    let screen = render(&world.app);
    assert!(
        screen.contains("o/site") && screen.contains("o/app"),
        "{screen}"
    );
    assert!(screen.contains("private"), "{screen}");

    // Typing narrows, choosing the first match.
    typed(&mut world.app, "app");
    let Mode::Dialog(Dialog::CloneProject(choice)) = &world.app.mode else {
        panic!("expected the clone dialog, got {:?}", world.app.mode);
    };
    assert_eq!(choice.shown().len(), 1);
    press(&mut world.app, KeyCode::Enter);

    // Then where: the folder picker, cloning.
    let Mode::Dialog(Dialog::AddProject { path, clone, .. }) = &world.app.mode else {
        panic!("expected the folder picker, got {:?}", world.app.mode);
    };
    assert_eq!(clone.as_deref(), Some("o/app"));
    assert_eq!(path.text(), "/src/");
    assert!(render(&world.app).contains("Folder to clone o/app into (as app/ there)"));
    let actions = press(&mut world.app, KeyCode::Enter);
    let (_, sent) = command(&actions);
    assert_eq!(
        sent,
        Command::CloneProject {
            repository: "o/app".to_owned(),
            into: PathBuf::from("/src/"),
        }
    );
    assert_eq!(world.app.mode, Mode::Browse);
}

#[test]
fn a_repository_named_is_cloned_though_github_could_not_list_any() {
    use muster_core::protocol::{ErrorBody, ErrorCode, ServerFrame};

    let mut world = world();
    let (req_id, _) = command(&press(&mut world.app, KeyCode::Char('g')));
    world.app.on_frame(ServerFrame::Reply {
        req_id,
        outcome: Err(ErrorBody::new(
            ErrorCode::BadRequest,
            "the GitHub CLI (gh) is not signed in; run `gh auth login`",
        )),
    });
    assert!(render(&world.app).contains("not signed in"));
    // Nothing chosen and nothing named: ⏎ says what it wants.
    press(&mut world.app, KeyCode::Enter);
    assert!(render(&world.app).contains("Choose one, or type its name"));
    typed(&mut world.app, "https://github.com/x/y");
    press(&mut world.app, KeyCode::Enter);
    let Mode::Dialog(Dialog::AddProject { clone, .. }) = &world.app.mode else {
        panic!("expected the folder picker, got {:?}", world.app.mode);
    };
    assert_eq!(clone.as_deref(), Some("x/y"));
}

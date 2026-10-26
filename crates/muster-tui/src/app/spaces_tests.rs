//! Spaces and free terminals in the terminal UI (`ARC-33`).

use crossterm::event::KeyCode;
use muster_core::protocol::{Command, Delta, ErrorBody, ErrorCode, Reply, ServerFrame};
use muster_core::spaces::Member;
use muster_core::{Space, SpaceId, Timestamp};

use super::super::tests::{World, command, press, render, typed, world};
use super::super::{Action, Confirmed, Dialog, Mode};
use crate::model::Row;
use crate::model::tests::free_terminal;

fn reply(world: &mut World, req_id: u64, outcome: Result<Reply, ErrorBody>) -> Vec<Action> {
    world.app.on_frame(ServerFrame::Reply { req_id, outcome })
}

fn delta(world: &mut World, delta: Delta) {
    world.app.on_frame(ServerFrame::Delta(delta));
}

fn space(name: &str, members: Vec<Member>) -> Space {
    Space {
        id: SpaceId::new(),
        name: name.to_owned(),
        members,
        created_at: Timestamp::from_millis(1),
    }
}

/// What the open menu offers, by what each says.
fn offered(world: &World) -> Vec<String> {
    match &world.app.mode {
        Mode::Menu(menu) => menu.items.iter().map(|item| item.what.clone()).collect(),
        other => panic!("no menu: {other:?}"),
    }
}

/// Chooses the open menu's item that says `what`.
fn choose(world: &mut World, what: &str) -> Vec<Action> {
    let at = offered(world)
        .iter()
        .position(|item| item == what)
        .unwrap_or_else(|| panic!("no {what:?} in {:?}", offered(world)));
    for _ in 0..at {
        press(&mut world.app, KeyCode::Down);
    }
    press(&mut world.app, KeyCode::Enter)
}

#[test]
fn a_space_is_made_from_its_menu_and_its_heading_selected_among_every_space_arc_33() {
    let mut world = world();
    press(&mut world.app, KeyCode::Char('W'));
    assert_eq!(
        offered(&world),
        ["everything", "new space…", "move app to a new space…"]
    );
    choose(&mut world, "new space…");
    assert!(matches!(
        world.app.mode,
        Mode::Dialog(Dialog::SpaceName { space: None, .. })
    ));
    assert!(render(&world.app).contains("it doesn't change where they run"));
    typed(&mut world.app, " Work ");
    let (req_id, sent) = command(&press(&mut world.app, KeyCode::Enter));
    assert_eq!(
        sent,
        Command::CreateSpace {
            name: "Work".to_owned()
        }
    );
    let home = space("Home", Vec::new());
    delta(&mut world, Delta::SpaceUpserted(home.clone()));
    // The reply may come before the space does.
    let work = space("Work", Vec::new());
    reply(&mut world, req_id, Ok(Reply::SpaceCreated { id: work.id }));
    delta(&mut world, Delta::SpaceUpserted(work.clone()));
    // Never narrowed to it: every space is listed, its heading selected.
    assert_eq!(world.app.space_id(), None);
    assert!(world.app.rows().contains(&Row::Space(home.id)));
    assert_eq!(world.app.selected, Some(Row::Space(work.id)));
    let shown = render(&world.app);
    assert!(!shown.contains("Sessions · Work"), "{shown}");
    assert!(shown.contains("Home") && shown.contains("Work"), "{shown}");
}

#[test]
fn a_space_shown_alone_says_the_way_back_to_every_space_arc_33() {
    let mut world = world();
    let work = space("Work", Vec::new());
    delta(&mut world, Delta::SpaceUpserted(work.clone()));
    press(&mut world.app, KeyCode::Char(']'));
    assert_eq!(world.app.space_id(), Some(work.id));
    assert!(render(&world.app).contains("everything shows every space"));
}

#[test]
fn the_list_shows_the_space_and_brackets_go_round_them() {
    let mut world = world();
    let project = world.project.id;
    let work = space("Work", vec![Member::Project(project)]);
    let home = space("Home", Vec::new());
    delta(&mut world, Delta::SpaceUpserted(work.clone()));
    delta(&mut world, Delta::SpaceUpserted(home.clone()));
    press(&mut world.app, KeyCode::Char(']'));
    assert_eq!(world.app.space_id(), Some(work.id));
    assert!(world.app.rows().contains(&Row::Project(project)));
    press(&mut world.app, KeyCode::Char(']'));
    assert_eq!(world.app.space_id(), Some(home.id));
    assert!(world.app.rows().is_empty());
    press(&mut world.app, KeyCode::Char(']'));
    assert_eq!(
        world.app.space_id(),
        None,
        "everything comes after the last"
    );
    press(&mut world.app, KeyCode::Char('['));
    assert_eq!(world.app.space_id(), Some(home.id));
    // Removed elsewhere: the list shows everything.
    delta(&mut world, Delta::SpaceRemoved { id: home.id });
    assert_eq!(world.app.space_id(), None);
    assert!(world.app.rows().contains(&Row::Project(project)));
}

#[test]
fn a_project_is_moved_to_another_space_or_to_a_new_one_from_the_menu() {
    let mut world = world();
    let project = world.project.id;
    let work = space("Work", vec![Member::Project(project)]);
    let home = space("Home", Vec::new());
    delta(&mut world, Delta::SpaceUpserted(work.clone()));
    delta(&mut world, Delta::SpaceUpserted(home.clone()));
    world.app.selected = Some(Row::Project(project));
    press(&mut world.app, KeyCode::Char('W'));
    assert!(!offered(&world).contains(&"move app to Work".to_owned()));
    let (_, sent) = command(&choose(&mut world, "move app to Home"));
    assert_eq!(
        sent,
        Command::MoveToSpace {
            space: home.id,
            member: Member::Project(project)
        }
    );
    press(&mut world.app, KeyCode::Char('W'));
    choose(&mut world, "move app to a new space…");
    typed(&mut world.app, "Client");
    let (req_id, sent) = command(&press(&mut world.app, KeyCode::Enter));
    assert_eq!(
        sent,
        Command::CreateSpace {
            name: "Client".to_owned()
        }
    );
    let client = SpaceId::new();
    let (_, sent) = command(&reply(
        &mut world,
        req_id,
        Ok(Reply::SpaceCreated { id: client }),
    ));
    assert_eq!(
        sent,
        Command::MoveToSpace {
            space: client,
            member: Member::Project(project)
        }
    );
}

#[test]
fn with_no_space_a_terminal_asks_for_one_first_then_starts_in_it() {
    let mut world = world();
    assert!(press(&mut world.app, KeyCode::Char('+')).is_empty());
    assert!(matches!(
        world.app.mode,
        Mode::Dialog(Dialog::SpaceName {
            space: None,
            then: Some(super::Later::Terminal),
            ..
        })
    ));
    assert!(render(&world.app).contains("Everything lives in a space"));
    typed(&mut world.app, "Home");
    let (req_id, _) = command(&press(&mut world.app, KeyCode::Enter));
    let home = SpaceId::new();
    let (_, sent) = command(&reply(
        &mut world,
        req_id,
        Ok(Reply::SpaceCreated { id: home }),
    ));
    let Command::StartSession {
        workspace, space, ..
    } = sent
    else {
        panic!("{sent:?}");
    };
    assert_eq!((workspace, space), (None, Some(home)));
}

#[test]
fn with_no_space_adding_a_project_asks_for_one_first_then_opens_though_the_reply_comes_first() {
    let mut world = world();
    assert!(press(&mut world.app, KeyCode::Char('a')).is_empty());
    assert!(matches!(
        world.app.mode,
        Mode::Dialog(Dialog::SpaceName {
            then: Some(super::Later::AddProject),
            ..
        })
    ));
    typed(&mut world.app, "Home");
    let (req_id, _) = command(&press(&mut world.app, KeyCode::Enter));
    // The reply before the space's delta: the dialog opens all the same.
    reply(
        &mut world,
        req_id,
        Ok(Reply::SpaceCreated { id: SpaceId::new() }),
    );
    assert!(
        matches!(world.app.mode, Mode::Dialog(Dialog::AddProject { .. })),
        "{:?}",
        world.app.mode
    );
}

#[test]
fn everything_lists_each_space_under_its_heading_which_folds_and_offers_what_goes_in_it() {
    let mut world = world();
    let project = world.project.id;
    let free = free_terminal("zsh", 9);
    delta(&mut world, Delta::SessionUpserted(free.clone()));
    let work = space("Work", vec![Member::Project(project)]);
    let home = space("Home", vec![Member::Session(free.id)]);
    delta(&mut world, Delta::SpaceUpserted(work.clone()));
    delta(&mut world, Delta::SpaceUpserted(home.clone()));
    let rows = world.app.rows();
    assert_eq!(rows[0], Row::Space(work.id));
    assert_eq!(rows[1], Row::Project(project));
    assert_eq!(
        rows[rows.len() - 2..],
        [Row::Space(home.id), Row::Session(free.id)]
    );
    let drawn = render(&world.app);
    assert!(drawn.contains("Work") && drawn.contains("Home"), "{drawn}");

    // A terminal goes in the selected line's space.
    world.app.selected = Some(Row::Session(free.id));
    let (_, sent) = command(&press(&mut world.app, KeyCode::Char('+')));
    assert!(matches!(sent, Command::StartSession { space, .. } if space == Some(home.id)));

    // Its heading folds what it holds away, and back.
    world.app.mode = Mode::Browse;
    world.app.selected = Some(Row::Space(work.id));
    press(&mut world.app, KeyCode::Char('e'));
    assert_eq!(
        world.app.rows()[..2],
        [Row::Space(work.id), Row::Space(home.id)]
    );
    press(&mut world.app, KeyCode::Char('e'));
    assert_eq!(world.app.rows()[1], Row::Project(project));

    // Its menu: what can go in it, renaming and removing it.
    press(&mut world.app, KeyCode::Char('W'));
    let menu = offered(&world);
    for item in [
        "add a project in Work…",
        "clone from GitHub in Work…",
        "a terminal in Work…",
        "rename Work…",
        "remove Work…",
    ] {
        assert!(menu.contains(&item.to_owned()), "{item} not in {menu:?}");
    }
    let (_, sent) = command(&choose(&mut world, "a terminal in Work…"));
    assert!(matches!(sent, Command::StartSession { space, .. } if space == Some(work.id)));
}

#[test]
fn f_files_a_free_terminal_where_its_shell_is_or_starts_it_again_after_asking() {
    let mut world = world();
    let free = free_terminal("npm", 9);
    delta(&mut world, Delta::SessionUpserted(free.clone()));
    world.app.selected = Some(Row::Session(free.id));
    let worktree = world.worktree.id;
    let (req_id, sent) = command(&press(&mut world.app, KeyCode::Char('F')));
    assert_eq!(sent, Command::ReadPlace { session: free.id });

    // Its shell is in the project's worktree: filed there, as it is.
    reply(
        &mut world,
        req_id,
        Ok(Reply::Place {
            folder: Some("/w/work/src".into()),
            workspace: Some(worktree),
            front: Some("npm".to_owned()),
        }),
    );
    let (_, sent) = command(&choose(&mut world, "file under app (work), as it is"));
    assert_eq!(
        sent,
        Command::FileSession {
            session: free.id,
            workspace: worktree
        }
    );

    // Elsewhere, in no project: offered as a project, or started again.
    let (req_id, _) = command(&press(&mut world.app, KeyCode::Char('F')));
    reply(
        &mut world,
        req_id,
        Ok(Reply::Place {
            folder: Some("/tmp/tool".into()),
            workspace: None,
            front: Some("npm".to_owned()),
        }),
    );
    assert!(
        offered(&world).contains(&"add /tmp/tool as a project, the terminal under it".to_owned())
    );
    choose(&mut world, "start again in app…");
    let Mode::Dialog(Dialog::Confirm { question, then }) = &world.app.mode else {
        panic!("{:?}", world.app.mode);
    };
    assert!(question.contains("npm ends"), "{question}");
    assert!(matches!(then, Confirmed::RestartIn { session, .. } if *session == free.id));
}

#[test]
fn a_refusal_about_spaces_is_said() {
    let mut world = world();
    press(&mut world.app, KeyCode::Char('W'));
    choose(&mut world, "new space…");
    typed(&mut world.app, "Work");
    let (req_id, _) = command(&press(&mut world.app, KeyCode::Enter));
    reply(
        &mut world,
        req_id,
        Err(ErrorBody::new(
            ErrorCode::Conflict,
            "there is a space called Work already",
        )),
    );
    let note = world.app.note.clone().expect("a note");
    assert!(note.error && note.text.contains("called Work already"));
}

#[test]
fn a_waiting_session_s_space_is_noted_in_the_queue() {
    let mut world = world();
    let project = world.project.id;
    let work = space("Work", vec![Member::Project(project)]);
    delta(&mut world, Delta::SpaceUpserted(work.clone()));
    let mut asking = world.session.clone();
    asking.harness = "claude".to_owned();
    asking.status = muster_core::SessionStatus::NeedsYou {
        reason: muster_core::WaitReason::Question,
    };
    delta(&mut world, Delta::SessionUpserted(asking));
    world.app.on_resize(muster_core::protocol::TermSize {
        cols: 200,
        rows: 40,
    });
    world.app.show(super::super::View::NeedsYou);
    assert!(
        render(&world.app).contains("in Work"),
        "{}",
        render(&world.app)
    );
}

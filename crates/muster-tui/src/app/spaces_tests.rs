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
fn a_space_is_made_from_its_menu_and_shown_once_it_is_there() {
    let mut world = world();
    press(&mut world.app, KeyCode::Char('W'));
    assert_eq!(offered(&world), ["everything", "new space…"]);
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
    let work = space("Work", Vec::new());
    delta(&mut world, Delta::SpaceUpserted(work.clone()));
    reply(&mut world, req_id, Ok(Reply::SpaceCreated { id: work.id }));
    assert_eq!(world.app.space_id(), Some(work.id));
    assert!(render(&world.app).contains("Sessions · Work"));
    // Empty: the project is in no space.
    assert!(world.app.rows().is_empty());
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
fn a_project_is_put_in_a_space_and_taken_out_from_the_menu() {
    let mut world = world();
    let project = world.project.id;
    let work = space("Work", Vec::new());
    delta(&mut world, Delta::SpaceUpserted(work.clone()));
    world.app.selected = Some(Row::Project(project));
    press(&mut world.app, KeyCode::Char('W'));
    let (_, sent) = command(&choose(&mut world, "put app in Work"));
    assert_eq!(
        sent,
        Command::AddToSpace {
            space: work.id,
            member: Member::Project(project)
        }
    );
    delta(
        &mut world,
        Delta::SpaceUpserted(Space {
            members: vec![Member::Project(project)],
            ..work.clone()
        }),
    );
    press(&mut world.app, KeyCode::Char('W'));
    let (_, sent) = command(&choose(&mut world, "take app out of Work"));
    assert_eq!(
        sent,
        Command::RemoveFromSpace {
            space: work.id,
            member: Member::Project(project)
        }
    );
}

#[test]
fn a_free_terminal_starts_in_the_space_shown_and_is_listed_under_terminals() {
    let mut world = world();
    let work = space("Work", Vec::new());
    delta(&mut world, Delta::SpaceUpserted(work.clone()));
    world.app.space = Some(work.id);
    let (_, sent) = command(&press(&mut world.app, KeyCode::Char('+')));
    let Command::StartSession {
        workspace, space, ..
    } = sent
    else {
        panic!("{sent:?}");
    };
    assert_eq!((workspace, space), (None, Some(work.id)));

    world.app.space = None;
    let free = free_terminal("zsh", 9);
    delta(&mut world, Delta::SessionUpserted(free.clone()));
    let rows = world.app.rows();
    assert_eq!(
        rows[rows.len() - 2..],
        [Row::Terminals, Row::Session(free.id)]
    );
    assert!(render(&world.app).contains("Terminals"));
    // Folded away, and back.
    world.app.selected = Some(Row::Terminals);
    press(&mut world.app, KeyCode::Char('e'));
    assert_eq!(world.app.rows().last(), Some(&Row::Terminals));
    press(&mut world.app, KeyCode::Char('e'));
    assert_eq!(world.app.rows().last(), Some(&Row::Session(free.id)));
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
fn a_project_s_spaces_are_noted_with_everything_and_a_waiting_session_s_in_the_queue() {
    let mut world = world();
    let project = world.project.id;
    let work = space("Work", vec![Member::Project(project)]);
    delta(&mut world, Delta::SpaceUpserted(work.clone()));
    assert!(render(&world.app).contains("app  in Work"));
    // In the space, it goes without saying.
    world.app.space = Some(work.id);
    assert!(!render(&world.app).contains("in Work"));

    world.app.space = None;
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

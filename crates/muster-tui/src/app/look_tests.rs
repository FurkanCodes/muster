//! The livelier look: what each agent is doing and for how long, the
//! counts in the top bar, each project's colour, the selection, notes that
//! fade, and screens with nothing on them yet.

use std::path::PathBuf;

use crossterm::event::KeyCode;
use muster_core::protocol::{ClientFrame, Delta, ResourceReport, ServerFrame, Snapshot};
use muster_core::{Activity, SessionId, SessionStatus, Timestamp, WaitReason};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::{Color, Modifier};

use super::tests::{SIZE, World, press, render, world};
use super::{Action, App, View};
use crate::model::Row;
use crate::model::tests::{project, session, worktree};
use crate::view::look::WORDMARK;

const NOW: i64 = 1_000_000_000;

fn ago(millis: i64) -> Timestamp {
    Timestamp::from_millis(NOW - millis)
}

fn status(world: &mut World, session: SessionId, status: SessionStatus, since_ago: i64) {
    world.app.on_frame(ServerFrame::Delta(Delta::SessionStatus {
        session,
        status,
        unseen: false,
        since: ago(since_ago),
    }));
}

fn doing(world: &mut World, session: SessionId, text: Option<&str>) {
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::SessionActivity {
            session,
            activity: text.map(|text| Activity {
                text: text.to_owned(),
                since: ago(0),
            }),
        }));
}

/// The screen's line holding `text`.
fn line_with(screen: &str, text: &str) -> String {
    screen
        .lines()
        .find(|line| line.contains(text))
        .unwrap_or_else(|| panic!("no line with {text:?} in\n{screen}"))
        .to_owned()
}

pub(super) fn buffer(app: &App) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(SIZE.cols, SIZE.rows + 1)).unwrap();
    terminal
        .draw(|frame| crate::view::draw(frame, app))
        .unwrap();
    terminal.backend().buffer().clone()
}

#[test]
fn a_working_agent_says_what_it_does_and_for_how_long() {
    let mut world = world();
    let id = world.session.id;
    world.app.on_clock(Timestamp::from_millis(NOW));
    status(&mut world, id, SessionStatus::Running, 252_000);
    doing(&mut world, id, Some("running npm"));
    let line = line_with(&render(&world.app), "running npm");
    // The time at the line's end, against the list's border.
    assert!(line.contains("❯ Shell  running npm"), "{line}");
    assert!(line.contains("4m12s│"), "{line}");
    // Still, it reads to the minute.
    world.app.motion = false;
    assert!(line_with(&render(&world.app), "running npm").contains("4m│"));
    world.app.motion = true;
    // Where it does not fit, what it does gives way; the time stays.
    doing(&mut world, id, Some("editing view_settings.rs"));
    let line = line_with(&render(&world.app), "editing");
    assert!(line.contains("❯ Shell  editing vi… 4m12s"), "{line}");
    assert!(line.contains("4m12s│"), "{line}");

    // Waiting on the person: what it wants, in red, and how long it has
    // waited; the same in its pane's title.
    status(
        &mut world,
        id,
        SessionStatus::NeedsYou {
            reason: WaitReason::Permission,
        },
        131_000,
    );
    doing(&mut world, id, Some("wants to run rm"));
    world.app.selected = Some(Row::Session(id));
    // Past the flash its line gives as it comes to wait.
    world.app.on_step(world.app.step + 20);
    let screen = render(&world.app);
    assert!(line_with(&screen, "Shell  wants to").contains("2m11s│"));
    assert!(
        screen.contains(" Shell · wants to run rm · 2m11s "),
        "{screen}"
    );
    let cells = buffer(&world.app);
    let row = (0..cells.area.height)
        .find(|y| {
            (0..cells.area.width)
                .map(|x| cells[(x, *y)].symbol())
                .collect::<String>()
                .contains("Shell  wants to")
        })
        .unwrap();
    let start = (0..cells.area.width)
        .find(|x| cells[(*x, row)].symbol() == "w" && cells[(*x + 1, row)].symbol() == "a")
        .unwrap();
    assert_eq!(cells[(start, row)].fg, Color::Red);

    // Finished: its status, and how long ago.
    status(&mut world, id, SessionStatus::Finished, 720_000);
    doing(&mut world, id, None);
    let line = line_with(&render(&world.app), "Shell  finished");
    assert!(line.contains("12m ago│"), "{line}");
}

#[test]
fn an_activity_once_the_agent_is_done_is_not_shown() {
    let mut world = world();
    let id = world.session.id;
    status(&mut world, id, SessionStatus::Running, 0);
    doing(&mut world, id, Some("running cargo"));
    status(&mut world, id, SessionStatus::Finished, 0);
    // The daemon clears it too; its line says what is so either way.
    let screen = render(&world.app);
    assert!(!screen.contains("running cargo"), "{screen}");
    assert!(screen.contains("Shell  finished"), "{screen}");
}

#[test]
fn the_top_bar_counts_the_agents_working_waiting_and_done() {
    let mut app = App::new(PathBuf::from("/src"), SIZE);
    app.on_connected("0.1.0".to_owned());
    let project = project("app");
    let worktree = worktree(project.id, "work");
    let statuses = [
        SessionStatus::Running,
        SessionStatus::Running,
        SessionStatus::NeedsYou {
            reason: WaitReason::Question,
        },
        SessionStatus::Finished,
        SessionStatus::Exited { success: true },
    ];
    let sessions = statuses
        .into_iter()
        .enumerate()
        .map(|(at, status)| {
            let mut found = session(worktree.id, "Agent", 3 + i64::try_from(at).unwrap());
            found.status = status;
            found
        })
        .collect();
    app.on_frame(ServerFrame::Snapshot(Snapshot {
        projects: vec![project],
        workspaces: vec![worktree],
        sessions,
        ..Snapshot::default()
    }));
    let top = render(&app).lines().next().unwrap().to_owned();
    assert!(top.contains("⠋ 2 working  ▲ 1 waiting  ✓ 1 done"), "{top}");
    // With the memory too, the counts give up their words to fit.
    app.on_frame(ServerFrame::Delta(Delta::Resources(ResourceReport {
        host_memory_used: 15_000_000_000,
        host_memory_total: 16_000_000_000,
        sessions: Vec::new(),
    })));
    let top = render(&app).lines().next().unwrap().to_owned();
    assert!(top.contains("⠋ 2  ▲ 1  ✓ 1   ▕███████▌▏"), "{top}");
    let cells = buffer(&app);
    let bar = top.find("███").unwrap();
    let column = u16::try_from(top[..bar].chars().count()).unwrap();
    assert_eq!(cells[(column, 0)].fg, Color::Red, "past 90% used");
}

#[test]
fn each_project_has_its_colour_and_the_selection_a_tint_and_a_bar() {
    let mut world = world();
    let other = project("web");
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::ProjectUpserted(other)));
    let cells = buffer(&world.app);
    let row_of = |name: &str| {
        (0..cells.area.height)
            .find(|y| {
                let line = (0..cells.area.width)
                    .map(|x| cells[(x, *y)].symbol())
                    .collect::<String>();
                ["█", "▌"]
                    .iter()
                    .any(|edge| line.contains(&format!("{edge}▾ {name}")))
            })
            .unwrap()
    };
    let (app_row, web_row) = (row_of("app"), row_of("web"));
    // The first project is selected: its edge is the selection's bar, and
    // its line is tinted rather than reversed.
    assert_eq!(cells[(1, app_row)].symbol(), "█");
    assert_eq!(cells[(1, web_row)].symbol(), "▌");
    assert_eq!(cells[(1, app_row)].fg, Color::Blue);
    assert_eq!(cells[(1, web_row)].fg, Color::Magenta);
    assert_eq!(
        cells[(4, app_row)].fg,
        Color::Blue,
        "its name in its colour, after its triangle"
    );
    assert_eq!(cells[(5, app_row)].bg, Color::DarkGray);
    assert!(!cells[(5, app_row)].modifier.contains(Modifier::REVERSED));
    assert_eq!(cells[(5, web_row)].bg, Color::Reset);
    // Its worktree and sessions carry its edge too.
    let line = line_with(&render(&world.app), "Shell");
    assert!(line.starts_with("│▌   ○ ❯ Shell"), "{line}");
}

#[test]
fn each_line_of_needs_you_names_its_project_in_its_colour() {
    let mut app = App::new(PathBuf::from("/src"), SIZE);
    app.on_connected("0.1.0".to_owned());
    let (mine, theirs) = (project("app"), project("web"));
    let (work, fix) = (worktree(mine.id, "work"), worktree(theirs.id, "fix-login"));
    let waiting = |workspace, created| {
        let mut found = session(workspace, "Claude Code", created);
        found.status = SessionStatus::NeedsYou {
            reason: WaitReason::Question,
        };
        found
    };
    app.on_frame(ServerFrame::Snapshot(Snapshot {
        projects: vec![mine, theirs.clone()],
        workspaces: vec![work.clone(), fix.clone()],
        sessions: vec![waiting(work.id, 3), waiting(fix.id, 4)],
        ..Snapshot::default()
    }));
    app.view = View::NeedsYou;
    // The same agent waits in both: only the project tells them apart,
    // and it stays where what each wants is cut short.
    let screen = render(&app);
    assert!(line_with(&screen, "▲ ❯ app  Claude Code").contains("needs yo"));
    assert!(line_with(&screen, "▲ ❯ web  Claude Code").contains("needs yo"));

    let cells = buffer(&app);
    let (row, column) = (0..cells.area.height)
        .find_map(|y| {
            let text: String = (0..cells.area.width)
                .map(|x| cells[(x, y)].symbol())
                .collect();
            let at = text.find("web  Claude")?;
            Some((y, u16::try_from(text[..at].chars().count()).unwrap()))
        })
        .unwrap();
    assert_eq!(
        cells[(column, row)].fg,
        crate::view::look::project_colour(&app, theirs.id)
    );
    assert!(cells[(column, row)].modifier.contains(Modifier::BOLD));
}

#[test]
fn notes_fade_and_errors_wait_for_a_key() {
    let mut world = world();
    world.app.on_step(100);
    world.app.say("Closed APP-7 in Jira.");
    assert!(!world.app.on_step(120), "nothing to draw while it is read");
    assert!(render(&world.app).contains("Closed APP-7 in Jira."));
    assert!(world.app.on_step(140), "it dims");
    assert!(world.app.note_fading());
    assert!(world.app.on_step(150), "and goes");
    assert!(world.app.note.is_none());
    assert!(render(&world.app).contains("↑↓ move"));

    world.app.complain("Jira refused (403).");
    world.app.on_step(400);
    assert!(render(&world.app).contains("Jira refused (403)."));
    press(&mut world.app, KeyCode::Down);
    assert!(world.app.note.is_none(), "gone with the next key");
}

#[test]
fn the_clock_asks_for_drawing_only_as_a_time_on_screen_changes() {
    let mut world = world();
    let id = world.session.id;
    assert!(world.app.on_clock(Timestamp::from_millis(NOW)), "the first");
    // Only an idle session: its time reads to the minute.
    assert!(!world.app.on_clock(Timestamp::from_millis(NOW + 1_000)));
    assert!(world.app.on_clock(Timestamp::from_millis(NOW + 60_000)));
    // One that runs counts its seconds.
    status(&mut world, id, SessionStatus::Running, 0);
    assert!(world.app.on_clock(Timestamp::from_millis(NOW + 61_000)));
    world.app.motion = false;
    assert!(!world.app.on_clock(Timestamp::from_millis(NOW + 62_000)));
    assert!(world.app.on_clock(Timestamp::from_millis(NOW + 120_000)));
}

#[test]
fn screens_with_nothing_on_them_yet_say_so_with_character() {
    let mut app = App::new(PathBuf::from("/src"), SIZE);
    assert_eq!(
        app.on_connected("0.1.0".to_owned()),
        [
            Action::Send(ClientFrame::Subscribe),
            Action::Send(ClientFrame::Looking { looking: true })
        ]
    );
    let screen = render(&app);
    assert!(screen.contains(WORDMARK[0]), "{screen}");
    assert!(screen.contains("Connecting to the daemon…"), "{screen}");
    app.on_frame(ServerFrame::Snapshot(Snapshot::default()));
    let screen = render(&app);
    assert!(screen.contains(WORDMARK[2]), "{screen}");
    assert!(
        screen.contains("Agents on your tickets, side by side."),
        "{screen}"
    );

    let mut world = world();
    world.app.view = View::NeedsYou;
    let screen = render(&world.app);
    assert!(screen.contains("✓ All clear"), "{screen}");
    assert!(screen.contains("Nothing is waiting on you."), "{screen}");
}

#[test]
fn a_ui_older_than_its_daemon_says_how_to_start_the_new_one() {
    let mut world = world();
    world.app.on_connected("999.0.0".to_owned());
    let screen = render(&world.app);
    let warning = line_with(&screen, "This window runs muster");
    assert!(warning.contains("the daemon is 999.0.0"), "{screen}");
    // Under it, the hints still.
    assert!(screen.contains("q quit"), "{screen}");
    let cells = buffer(&world.app);
    let row = (0..cells.area.height)
        .find(|y| {
            (0..cells.area.width)
                .map(|x| cells[(x, *y)].symbol())
                .collect::<String>()
                .contains("This window runs")
        })
        .unwrap();
    assert_eq!(cells[(1, row)].fg, Color::Yellow);

    // In a session, the session keeps its size and the bar its one line.
    world.app.selected = Some(Row::Session(world.session.id));
    press(&mut world.app, KeyCode::Enter);
    let screen = render(&world.app);
    assert!(!screen.contains("This window runs muster"), "{screen}");
    assert!(screen.contains("keys go to the session"), "{screen}");

    // A daemon of its own version, or older: nothing to say.
    world.app.on_key(super::tests::shift_q());
    for version in [env!("CARGO_PKG_VERSION"), "0.1.0"] {
        world.app.on_connected(version.to_owned());
        assert!(!render(&world.app).contains("This window runs muster"));
    }
}

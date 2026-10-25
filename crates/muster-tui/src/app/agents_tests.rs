//! The coding agents the person added in Muster's own settings: offered
//! wherever Muster's own are, for a new session and a ticket (where they
//! take its prompt), marked as their settings say, and shown on the
//! Settings screen with why they do not read where they do not.

use crossterm::event::KeyCode;
use muster_core::protocol::{Command, Delta, ServerFrame};
use muster_core::settings::{AddedAgent, Preferences};

use super::settings_tests::on_settings;
use super::tests::{PANE, World, command, press, render, typed, world};
use super::tickets_tests::{agent_on_42, commands, key, on_tickets};
use super::{Dialog, HarnessChoice, Mode, SettingRow};
use crate::model::Row;

fn my_agent() -> AddedAgent {
    AddedAgent {
        name: "my-agent".to_owned(),
        label: "My Agent".to_owned(),
        mark: "✦".to_owned(),
        colour: Some("green".to_owned()),
        takes_prompt: true,
        asks: false,
    }
}

fn quiet() -> AddedAgent {
    AddedAgent {
        name: "quiet".to_owned(),
        label: "Quiet".to_owned(),
        mark: "Q".to_owned(),
        colour: None,
        takes_prompt: false,
        asks: false,
    }
}

/// The daemon says Muster's own settings now hold `agents`, or why they
/// do not read.
fn told(world: &mut World, agents: Vec<AddedAgent>, problem: Option<&str>) {
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::Preferences(Preferences {
            file: "/home/me/.config/muster/settings.toml".into(),
            agents,
            problem: problem.map(str::to_owned),
            ..Preferences::default()
        })));
}

#[test]
fn a_new_session_offers_the_person_s_agents_after_muster_s_own() {
    let mut world = world();
    told(&mut world, vec![my_agent(), quiet()], None);
    world.app.selected = Some(Row::Worktree(world.worktree.id));
    press(&mut world.app, KeyCode::Char('n'));
    // In their order, the line wrapping where it is full.
    let screen = render(&world.app);
    for shown in [
        "Run  Claude Code  Codex  Gemini CLI  OpenCode  Copilot CLI  My Agent ",
        "Quiet  Shell ",
    ] {
        assert!(screen.contains(shown), "{shown:?} in\n{screen}");
    }
    for _ in 0..5 {
        press(&mut world.app, KeyCode::Tab);
    }
    assert!(render(&world.app).contains("First prompt"));
    typed(&mut world.app, "fix it");
    let (_, sent) = command(&press(&mut world.app, KeyCode::Enter));
    assert_eq!(
        sent,
        Command::StartSession {
            workspace: Some(world.worktree.id),
            harness: "my-agent".to_owned(),
            title: None,
            size: PANE,
            prompt: Some("fix it".to_owned()),
            started_by: None,
            space: None,
        }
    );

    // Offered first next time; one that takes no prompt is given none.
    press(&mut world.app, KeyCode::Char('n'));
    assert!(matches!(
        &world.app.mode,
        Mode::Dialog(Dialog::NewSession { harness: HarnessChoice::Added(agent), .. })
            if agent.name == "my-agent"
    ));
    press(&mut world.app, KeyCode::Tab);
    assert!(!render(&world.app).contains("First prompt"));
    typed(&mut world.app, "ignored");
    let (_, sent) = command(&press(&mut world.app, KeyCode::Enter));
    let Command::StartSession {
        harness, prompt, ..
    } = sent
    else {
        panic!("expected StartSession");
    };
    assert_eq!((harness.as_str(), prompt), ("quiet", None));
}

#[test]
fn a_ticket_is_offered_the_person_s_agents_that_take_its_prompt() {
    let mut world = on_tickets();
    told(&mut world, vec![my_agent(), quiet()], None);
    press(&mut world.app, KeyCode::Enter);
    let screen = render(&world.app);
    assert!(screen.contains("My Agent"), "{screen}");
    assert!(!screen.contains("Quiet"), "{screen}");
    // Back from Claude Code, round to the last agent offered: the
    // person's.
    press(&mut world.app, KeyCode::BackTab);
    let actions = press(&mut world.app, KeyCode::Enter);
    assert_eq!(
        commands(&actions),
        [Command::StartFromIssue {
            project: world.project.id,
            key: key(42),
            harness: "my-agent".to_owned(),
            size: PANE,
            another: false,
            on: None,
            brief: None,
        }]
    );

    // Its session on the ticket goes by its name and its mark.
    let mut agent = agent_on_42(&mut world, "muster/issue-42-fix-the-login-redirect", 10);
    world.app.mode = Mode::Browse;
    agent.harness = "my-agent".to_owned();
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::SessionUpserted(agent)));
    let screen = render(&world.app);
    assert!(screen.contains("My Agent  running"), "{screen}");
    assert!(screen.contains('✦'), "{screen}");
}

#[test]
fn the_settings_screen_shows_the_person_s_agents_or_why_they_do_not_read() {
    let mut world = on_settings(false);
    told(&mut world, vec![my_agent(), quiet()], None);
    world.app.settings.selected = Some(SettingRow::Agents);
    let screen = render(&world.app);
    for shown in [
        "Your agents",
        "My Agent, Quiet",
        "my-agent, takes a first prompt",
        "quiet, takes no prompt",
        "[agents.my-agent]",
        "/home/me/.config/muster/settings.toml",
    ] {
        assert!(screen.contains(shown), "{shown:?} in\n{screen}");
    }

    told(
        &mut world,
        Vec::new(),
        Some("settings.toml: agents.my-agent.color is not a setting of an agent"),
    );
    let screen = render(&world.app);
    for shown in [
        "cannot be read",
        "Muster's own settings cannot be read:",
        "agents.my-agent.color",
    ] {
        assert!(screen.contains(shown), "{shown:?} in\n{screen}");
    }
}

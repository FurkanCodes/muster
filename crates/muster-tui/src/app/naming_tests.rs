//! Chats named by an agent, on the Settings screen (`PRODUCT.md` §6): on
//! or off, which agent, and its model, with what it costs said beside it.

use crossterm::event::KeyCode;
use muster_core::protocol::{ClientFrame, Command, Reply, ServerFrame};
use muster_core::settings::{AddedAgent, NAMING_PROTOCOL, Naming, PreferenceChange};

use super::tests::{World, command, preferences, press, render, typed, world};
use super::{Action, Dialog, Mode, SettingRow};

fn on_row(world: &mut World, row: SettingRow) {
    press(&mut world.app, KeyCode::Char('4'));
    world.app.settings_selected = Some(row);
}

fn changed(actions: &[Action]) -> Option<PreferenceChange> {
    actions.iter().find_map(|action| match action {
        Action::Send(ClientFrame::Command {
            command: Command::ChangePreference(change),
            ..
        }) => Some(change.clone()),
        _ => None,
    })
}

fn named_by(world: &mut World, agent: Option<&str>, model: Option<&str>) {
    preferences(&mut world.app, |preferences| {
        preferences.naming = Naming {
            by_agent: true,
            agent: agent.map(str::to_owned),
            model: model.map(str::to_owned),
        };
    });
}

#[test]
fn naming_by_an_agent_is_on_by_default_and_says_what_it_costs() {
    let mut world = world();
    on_row(&mut world, SettingRow::NamesByAgent);
    let screen = render(&world.app);
    assert!(screen.contains("Chats named by an agent"), "{screen}");
    // Wrapped as the pane is wide: what it uses, and what it sends.
    assert!(screen.contains("your own sign-in"), "{screen}");
    assert!(screen.contains("credentials"), "{screen}");
    assert_eq!(
        changed(&press(&mut world.app, KeyCode::Enter)),
        Some(PreferenceChange::NamesByAgent(false))
    );
}

#[test]
fn a_daemon_that_names_no_chats_shows_none_of_it() {
    let mut world = world();
    world.app.daemon_protocol = NAMING_PROTOCOL - 1;
    let rows = world.app.setting_rows();
    for row in [
        SettingRow::NamesByAgent,
        SettingRow::NamedBy,
        SettingRow::NamingModel,
    ] {
        assert!(!rows.contains(&row), "{row:?}");
    }
}

#[test]
fn the_agents_that_can_be_asked_one_question_name_chats_in_turn() {
    let mut world = world();
    preferences(&mut world.app, |preferences| {
        preferences.agents = vec![
            AddedAgent {
                name: "pi".to_owned(),
                label: "Pi".to_owned(),
                mark: "P".to_owned(),
                colour: None,
                takes_prompt: false,
                asks: true,
            },
            AddedAgent {
                name: "quiet".to_owned(),
                label: "Quiet".to_owned(),
                mark: "Q".to_owned(),
                colour: None,
                takes_prompt: true,
                asks: false,
            },
        ];
    });
    let labels: Vec<String> = world
        .app
        .namers()
        .into_iter()
        .map(|(_, label)| label)
        .collect();
    assert_eq!(
        labels,
        ["each session's own agent", "Claude Code", "Codex", "Pi"]
    );
    on_row(&mut world, SettingRow::NamedBy);
    assert_eq!(
        changed(&press(&mut world.app, KeyCode::Enter)),
        Some(PreferenceChange::NamedBy {
            agent: Some("claude".to_owned()),
            model: None
        })
    );
    named_by(&mut world, Some("pi"), None);
    assert_eq!(
        changed(&press(&mut world.app, KeyCode::Enter)),
        Some(PreferenceChange::NamedBy {
            agent: None,
            model: None
        }),
        "round to each session's own"
    );
}

#[test]
fn the_model_is_picked_among_those_the_agent_offers_or_typed() {
    let mut world = world();
    on_row(&mut world, SettingRow::NamingModel);
    // Each session's own agent uses its own model.
    press(&mut world.app, KeyCode::Enter);
    assert_eq!(world.app.mode, Mode::Browse);
    assert!(world.app.note.as_ref().is_some_and(|note| note.error));

    named_by(&mut world, Some("claude"), None);
    let (req_id, asked) = command(&press(&mut world.app, KeyCode::Enter));
    assert_eq!(
        asked,
        Command::ListModels {
            agent: "claude".to_owned()
        }
    );
    assert!(render(&world.app).contains("Loading models"));
    world.app.on_frame(ServerFrame::Reply {
        req_id,
        outcome: Ok(Reply::Models(vec!["haiku".to_owned(), "sonnet".to_owned()])),
    });
    assert!(render(&world.app).contains("It offers: haiku, sonnet"));
    press(&mut world.app, KeyCode::Tab);
    press(&mut world.app, KeyCode::Tab);
    let Mode::Dialog(Dialog::NamingModel { model, .. }) = &world.app.mode else {
        panic!("{:?}", world.app.mode);
    };
    assert_eq!(model.text(), "sonnet");
    assert_eq!(
        changed(&press(&mut world.app, KeyCode::Enter)),
        Some(PreferenceChange::NamedBy {
            agent: Some("claude".to_owned()),
            model: Some("sonnet".to_owned())
        })
    );

    // One it does not list, typed; or none, for its own choice.
    named_by(&mut world, Some("claude"), Some("sonnet"));
    press(&mut world.app, KeyCode::Enter);
    for _ in 0.."sonnet".len() {
        press(&mut world.app, KeyCode::Backspace);
    }
    typed(&mut world.app, "claude-haiku-5-5");
    assert_eq!(
        changed(&press(&mut world.app, KeyCode::Enter)),
        Some(PreferenceChange::NamedBy {
            agent: Some("claude".to_owned()),
            model: Some("claude-haiku-5-5".to_owned())
        })
    );
    named_by(&mut world, Some("claude"), Some("x"));
    press(&mut world.app, KeyCode::Enter);
    press(&mut world.app, KeyCode::Backspace);
    assert_eq!(
        changed(&press(&mut world.app, KeyCode::Enter)),
        Some(PreferenceChange::NamedBy {
            agent: Some("claude".to_owned()),
            model: None
        })
    );
}

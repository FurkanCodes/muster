//! What each session's agent has used, as the daemon read it from the
//! agent's own records: in its pane's title and on its full screen.

use crossterm::event::KeyCode;
use muster_core::Spend;
use muster_core::protocol::{Delta, ServerFrame};

use super::tests::{press, render, world};
use crate::model::Row;

#[test]
fn a_session_s_pane_and_full_screen_say_what_its_agent_used() {
    let mut world = world();
    world.app.selected = Some(Row::Session(world.session.id));
    assert!(!render(&world.app).contains("tokens"), "nothing said yet");
    let mut spent = world.session.clone();
    spent.spend = Some(Spend {
        tokens: 1_234_567,
        cost_micros: None,
    });
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::SessionUpserted(spent.clone())));
    let screen = render(&world.app);
    assert!(screen.contains("· 1.2M tokens "), "{screen}");
    press(&mut world.app, KeyCode::Char('f'));
    let top = render(&world.app).lines().next().unwrap().to_owned();
    assert!(top.contains("· 1.2M tokens"), "{top}");
    // Where the agent reports what it cost, that too.
    spent.spend = Some(Spend {
        tokens: 40_000,
        cost_micros: Some(420_000),
    });
    world
        .app
        .on_frame(ServerFrame::Delta(Delta::SessionUpserted(spent)));
    let top = render(&world.app).lines().next().unwrap().to_owned();
    assert!(top.contains("· $0.42 · 40k tokens"), "{top}");
}

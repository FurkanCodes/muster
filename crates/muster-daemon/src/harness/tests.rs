//! The harness list, what each harness can do, and how each is started.

use std::path::PathBuf;

use muster_core::SessionId;
use muster_core::protocol::TermSize;
use muster_state::session::HookEvent;

use super::*;

#[test]
fn only_known_harnesses_are_found() {
    assert_eq!(find("shell"), Some(SHELL));
    assert_eq!(find("claude"), Some(CLAUDE));
    assert_eq!(find("checks"), Some(CHECKS));
    assert_eq!(find("review"), Some(REVIEW));
    assert_eq!(find("nonsense"), None);
}

#[test]
fn tokens_are_long_random_and_distinct() {
    let (one, two) = (new_token().unwrap(), new_token().unwrap());
    assert_eq!(one.len(), 64);
    assert!(one.chars().all(|c| c.is_ascii_hexdigit()));
    assert_ne!(one, two);
}

#[test]
fn checks_run_in_a_shell_with_their_limit_and_commands() {
    let spec = spawn_spec(
        CHECKS,
        Launch {
            session: SessionId::new(),
            agent_session: None,
            resume: false,
            prompt: None,
            hooks: None,
            place: Place::Host(PathBuf::from("/work")),
            size: TermSize::default(),
            checks: Some(CheckCommands {
                commands: vec!["cargo test".to_owned(), "cargo clippy".to_owned()],
                limit: std::time::Duration::from_secs(1800),
            }),
            support: None,
        },
    );
    assert_eq!(spec.program, "/bin/sh");
    assert_eq!(spec.args[0], "-c");
    assert_eq!(spec.args[2..], ["sh", "1800", "cargo test", "cargo clippy"]);
    assert_eq!(spec.cwd, PathBuf::from("/work"));
}

#[test]
fn a_review_is_claude_code_in_plan_mode_resumed_or_not() {
    let launch = |resume| Launch {
        session: SessionId::new(),
        agent_session: Some("id".to_owned()),
        resume,
        prompt: Some("Review the change".to_owned()),
        hooks: None,
        place: Place::Host(PathBuf::from("/work")),
        size: TermSize::default(),
        checks: None,
        support: None,
    };
    let fresh = spawn_spec(REVIEW, launch(false));
    assert_eq!(fresh.program, "claude");
    assert_eq!(fresh.args[..2], ["--permission-mode", "plan"]);
    assert_eq!(
        fresh.args.last().map(String::as_str),
        Some("Review the change")
    );
    let resumed = spawn_spec(REVIEW, launch(true));
    assert_eq!(
        resumed.args[..4],
        ["--permission-mode", "plan", "--resume", "id"]
    );
    assert!(REVIEW.is_agent() && REVIEW.can_resume() && REVIEW.has_hooks());
}

#[test]
fn hooks_reach_the_program_through_its_environment() {
    let spec = spawn_spec(
        CLAUDE,
        Launch {
            session: SessionId::new(),
            agent_session: Some("id".to_owned()),
            resume: false,
            prompt: None,
            hooks: Some(HookEndpoint {
                socket: PathBuf::from("/run/hooks.sock"),
                token: "secret".to_owned(),
            }),
            place: Place::Host(PathBuf::from("/work")),
            size: TermSize::default(),
            checks: None,
            support: None,
        },
    );
    assert_eq!(spec.program, "claude");
    let value = |key: &str| {
        spec.env
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    };
    assert_eq!(value("MUSTER_HOOK_SOCKET"), Some("/run/hooks.sock"));
    assert_eq!(value("MUSTER_HOOK_TOKEN"), Some("secret"));
}

#[test]
fn every_harness_is_found_by_its_id() {
    for harness in [
        SHELL, CLAUDE, CODEX, GEMINI, OPENCODE, COPILOT, CHECKS, REVIEW,
    ] {
        assert_eq!(find(harness.id), Some(harness));
    }
}

#[test]
fn each_agent_reports_the_way_it_allows() {
    let agents = [CLAUDE, REVIEW, CODEX, GEMINI, OPENCODE, COPILOT];
    for agent in agents {
        assert!(
            agent.is_agent() && agent.has_hooks() && agent.can_resume(),
            "{}",
            agent.id
        );
        assert!(
            agent.takes_prompt() && agent.suspends_itself(),
            "{}",
            agent.id
        );
        assert!(agent.hook_route().is_some(), "{}", agent.id);
    }
    for other in [SHELL, CHECKS] {
        assert!(!other.is_agent() && !other.has_hooks() && other.hook_route().is_none());
    }
    let choosing: Vec<&str> = agents
        .iter()
        .filter(|agent| agent.chooses_session_id())
        .map(|agent| agent.id)
        .collect();
    assert_eq!(choosing, ["claude", "review", "gemini", "copilot"]);
    let starting: Vec<&str> = agents
        .iter()
        .filter(|agent| agent.reports_start())
        .map(|agent| agent.id)
        .collect();
    assert_eq!(
        starting,
        ["claude", "review", "codex", "gemini", "opencode"]
    );
    assert!(CODEX.title_reports_waits() && !GEMINI.title_reports_waits());
    assert_eq!(CODEX.read_title("Ready"), Some(TitleState::Ready));
    assert_eq!(CLAUDE.read_title("Ready"), None);
}

#[test]
fn a_report_is_read_by_its_own_harness() {
    let stop = br#"{"session_id":"s1","hook_event_name":"Stop"}"#;
    assert_eq!(
        CLAUDE.translate(None, stop),
        Some((Some("s1".to_owned()), HookEvent::TurnEnded))
    );
    assert_eq!(
        COPILOT.translate(Some("agentStop"), br#"{"sessionId":"s2"}"#),
        Some((Some("s2".to_owned()), HookEvent::TurnEnded))
    );
    // Copilot names its events in the address only.
    assert_eq!(COPILOT.translate(None, br#"{"sessionId":"s2"}"#), None);
    assert_eq!(SHELL.translate(None, stop), None);
}

#[test]
fn hooks_post_to_their_route_and_are_inert_outside_muster() {
    let command = hook_command("gemini", None);
    assert!(command.starts_with(
        "[ -n \"$MUSTER_HOOK_TOKEN\" ] && [ -S \"$MUSTER_HOOK_SOCKET\" ] || exit 0; curl "
    ));
    assert!(
        command.contains("--data-binary @- http://muster/hooks/gemini >/dev/null 2>&1 || true")
    );
    assert!(
        hook_command("copilot", Some("agentStop"))
            .contains("http://muster/hooks/copilot/agentStop ")
    );
}

fn agent_launch(agent_session: Option<&str>, support: Option<&str>) -> Launch {
    Launch {
        session: SessionId::new(),
        agent_session: agent_session.map(str::to_owned),
        resume: false,
        prompt: Some("fix it".to_owned()),
        hooks: Some(HookEndpoint {
            socket: PathBuf::from("/run/hooks.sock"),
            token: "secret".to_owned(),
        }),
        place: Place::Host(PathBuf::from("/work")),
        size: TermSize::default(),
        checks: None,
        support: support.map(PathBuf::from),
    }
}

fn value<'a>(spec: &'a SpawnSpec, key: &str) -> Option<&'a str> {
    spec.env
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.as_str())
}

#[test]
fn each_agent_runs_its_own_program_pointed_at_muster_s_files() {
    let codex = spawn_spec(CODEX, agent_launch(None, Some("/run/support")));
    assert_eq!(codex.program, "codex");
    assert_eq!(codex.args[..3], ["--no-daemon", "-C", "/work"]);

    let gemini = spawn_spec(GEMINI, agent_launch(Some("g1"), Some("/run/support")));
    assert_eq!(gemini.program, "gemini");
    assert_eq!(gemini.args[..2], ["--session-id", "g1"]);
    if std::env::var_os(gemini::DEFAULTS_VARIABLE).is_none() {
        assert_eq!(
            value(&gemini, gemini::DEFAULTS_VARIABLE),
            Some("/run/support/gemini/defaults.json")
        );
    }

    let opencode = spawn_spec(OPENCODE, agent_launch(None, Some("/run/support")));
    assert_eq!(opencode.program, "opencode");
    assert_eq!(opencode.args, ["/work", "--prompt", "fix it"]);
    if std::env::var_os(opencode::CONFIG_VARIABLE).is_none() {
        assert_eq!(
            value(&opencode, opencode::CONFIG_VARIABLE),
            Some(r#"{"plugin":["file:///run/support/opencode/muster.js"]}"#)
        );
    }

    let copilot = spawn_spec(COPILOT, agent_launch(Some("c1"), Some("/run/support")));
    assert_eq!(copilot.program, "copilot");
    assert_eq!(
        copilot.args,
        [
            "--session-id=c1",
            "--plugin-dir",
            "/run/support/copilot",
            "--interactive",
            "fix it"
        ]
    );
    for spec in [&codex, &gemini, &opencode, &copilot] {
        assert_eq!(value(spec, "MUSTER_HOOK_TOKEN"), Some("secret"));
    }
}

#[test]
fn without_muster_s_files_an_agent_still_runs() {
    let gemini = spawn_spec(GEMINI, agent_launch(Some("g1"), None));
    assert_eq!(value(&gemini, gemini::DEFAULTS_VARIABLE), None);
    let copilot = spawn_spec(COPILOT, agent_launch(Some("c1"), None));
    assert!(!copilot.args.iter().any(|arg| arg == "--plugin-dir"));
}

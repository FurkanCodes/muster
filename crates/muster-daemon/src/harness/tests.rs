//! The harness list, what each harness can do, and how each is started.

use std::path::PathBuf;

use muster_core::SessionId;
use muster_core::protocol::TermSize;
use muster_state::session::HookEvent;

use super::*;

const SHELL: Harness = Harness { kind: Kind::Shell };
const CLAUDE: Harness = Harness { kind: Kind::Claude };
const CODEX: Harness = Harness { kind: Kind::Codex };
const GEMINI: Harness = Harness { kind: Kind::Gemini };
const OPENCODE: Harness = Harness {
    kind: Kind::OpenCode,
};
const COPILOT: Harness = Harness {
    kind: Kind::Copilot,
};
const CHECKS: Harness = Harness { kind: Kind::Checks };
const REVIEW: Harness = Harness { kind: Kind::Review };

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
        &CHECKS,
        &Launch {
            session: SessionId::new(),
            agent_session: None,
            resume: false,
            prompt: None,
            hooks: None,
            place: Place::Host(PathBuf::from("/work")),
            size: TermSize::default(),
            checks: Some(CheckCommands {
                commands: vec!["cargo test".to_owned(), "cargo clippy".to_owned()],
                limit: std::time::Duration::from_mins(30),
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
    let launch = |resume: bool| Launch {
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
    let fresh = spawn_spec(&REVIEW, &launch(false));
    assert_eq!(fresh.program, "claude");
    assert_eq!(fresh.args[..2], ["--permission-mode", "plan"]);
    assert_eq!(
        fresh.args.last().map(String::as_str),
        Some("Review the change")
    );
    let resumed = spawn_spec(&REVIEW, &launch(true));
    assert_eq!(
        resumed.args[..4],
        ["--permission-mode", "plan", "--resume", "id"]
    );
    assert!(REVIEW.is_agent() && REVIEW.can_resume() && REVIEW.has_hooks());
}

#[test]
fn hooks_reach_the_program_through_its_environment() {
    let spec = spawn_spec(
        &CLAUDE,
        &Launch {
            session: SessionId::new(),
            agent_session: Some("id".to_owned()),
            resume: false,
            prompt: None,
            hooks: Some(HookEndpoint {
                socket: PathBuf::from("/run/hooks.sock"),
                token: crate::secrets::Token::new("secret"),
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
        assert_eq!(find(harness.id()), Some(harness.clone()));
    }
}

#[test]
fn each_agent_reports_the_way_it_allows() {
    let agents = [CLAUDE, REVIEW, CODEX, GEMINI, OPENCODE, COPILOT];
    for agent in &agents {
        assert!(
            agent.is_agent() && agent.has_hooks() && agent.can_resume(),
            "{}",
            agent.id()
        );
        assert!(
            agent.takes_prompt() && agent.suspends_itself(),
            "{}",
            agent.id()
        );
        assert!(agent.hook_route().is_some(), "{}", agent.id());
    }
    for other in [SHELL, CHECKS] {
        assert!(!other.is_agent() && !other.has_hooks() && other.hook_route().is_none());
    }
    let choosing: Vec<&str> = agents
        .iter()
        .filter(|agent| agent.chooses_session_id())
        .map(Harness::id)
        .collect();
    assert_eq!(choosing, ["claude", "review", "gemini", "copilot"]);
    let starting: Vec<&str> = agents
        .iter()
        .filter(|agent| agent.reports_start())
        .map(Harness::id)
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

/// Shaped like what Claude Code 2.1.296 sent for an MCP server's form the
/// person pressed Esc at, in order; paths and the schema left out.
const ELICITED: [&str; 4] = [
    r#"{"session_id":"s1","prompt_id":"p1","hook_event_name":"Elicitation","mcp_server_name":"probe","message":"Which name?","mode":"form"}"#,
    r#"{"session_id":"s1","prompt_id":"p1","hook_event_name":"Notification","notification_type":"elicitation_dialog","message":"Claude Code needs your input"}"#,
    r#"{"session_id":"s1","prompt_id":"p1","hook_event_name":"ElicitationResult","mcp_server_name":"probe","action":"cancel","mode":"form"}"#,
    r#"{"session_id":"s1","prompt_id":"p1","hook_event_name":"Notification","notification_type":"elicitation_response","message":"Elicitation response for server \"probe\": cancel"}"#,
];

#[test]
fn claude_s_mcp_form_is_a_question_until_the_person_responds() {
    use muster_state::session::Origin;
    let heard: Vec<_> = ELICITED
        .iter()
        .map(|body| {
            CLAUDE
                .translate(None, body.as_bytes())
                .map(|(_, event)| event)
        })
        .collect();
    let asked = |deferred| {
        Some(HookEvent::QuestionAsked {
            origin: Origin::Main,
            deferred,
        })
    };
    let answered = Some(HookEvent::ToolFinished {
        origin: Origin::Main,
        question: true,
    });
    assert_eq!(
        heard,
        [asked(false), asked(true), answered.clone(), answered]
    );
    // A subagent's form is its own question, answered by its own result.
    let theirs = br#"{"session_id":"s1","agent_id":"a1","hook_event_name":"Elicitation","mcp_server_name":"probe"}"#;
    assert_eq!(
        CLAUDE.translate(None, theirs).map(|(_, event)| event),
        Some(HookEvent::QuestionAsked {
            origin: Origin::Subagent("a1".to_owned()),
            deferred: false,
        })
    );
    // Both hooks are passed at launch, unmatched (`PRD-13`).
    let settings: serde_json::Value = serde_json::from_str(&claude::settings()).unwrap();
    for event in ["Elicitation", "ElicitationResult"] {
        let group = &settings["hooks"][event][0];
        assert!(group.get("matcher").is_none(), "{event}");
        assert_eq!(group["hooks"][0]["command"], claude::hook_command());
    }
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
            token: crate::secrets::Token::new("secret"),
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
fn every_program_is_told_its_terminal_is_muster_on_the_host_and_in_a_container() {
    let in_container = Launch {
        place: Place::Container {
            docker: "docker".to_owned(),
            container: "run-1".to_owned(),
            secret: None,
        },
        ..agent_launch(Some("id"), None)
    };
    let mut specs: Vec<SpawnSpec> = [SHELL, CLAUDE, CODEX, GEMINI, OPENCODE, COPILOT]
        .into_iter()
        .map(|harness| spawn_spec(&harness, &agent_launch(Some("id"), None)))
        .collect();
    specs.push(spawn_spec(&SHELL, &in_container));
    specs.push(spawn_spec(&CLAUDE, &in_container));
    for spec in &specs {
        assert_eq!(value(spec, "TERM"), Some("xterm-256color"), "{spec:?}");
        assert_eq!(value(spec, "COLORTERM"), Some("truecolor"));
        assert_eq!(value(spec, "TERM_PROGRAM"), Some("muster"));
        assert_eq!(
            value(spec, "TERM_PROGRAM_VERSION"),
            Some(env!("CARGO_PKG_VERSION"))
        );
    }
    // In a container they reach the program through docker's environment.
    for spec in &specs[specs.len() - 2..] {
        let named = |key: &str| spec.args.windows(2).any(|pair| pair == ["--env", key]);
        assert!(
            named("TERM_PROGRAM") && named("TERM_PROGRAM_VERSION"),
            "{spec:?}"
        );
    }
}

#[test]
fn each_agent_runs_its_own_program_pointed_at_muster_s_files() {
    let codex = spawn_spec(&CODEX, &agent_launch(None, Some("/run/support")));
    assert_eq!(codex.program, "codex");
    assert_eq!(codex.args[..3], ["--no-daemon", "-C", "/work"]);

    let gemini = spawn_spec(&GEMINI, &agent_launch(Some("g1"), Some("/run/support")));
    assert_eq!(gemini.program, "gemini");
    assert_eq!(gemini.args[..2], ["--session-id", "g1"]);
    if std::env::var_os(gemini::DEFAULTS_VARIABLE).is_none() {
        assert_eq!(
            value(&gemini, gemini::DEFAULTS_VARIABLE),
            Some("/run/support/gemini/defaults.json")
        );
    }

    let opencode = spawn_spec(&OPENCODE, &agent_launch(None, Some("/run/support")));
    assert_eq!(opencode.program, "opencode");
    assert_eq!(opencode.args, ["/work", "--prompt", "fix it"]);
    if std::env::var_os(opencode::CONFIG_VARIABLE).is_none() {
        assert_eq!(
            value(&opencode, opencode::CONFIG_VARIABLE),
            Some(r#"{"plugin":["file:///run/support/opencode/muster.js"]}"#)
        );
    }

    let copilot = spawn_spec(&COPILOT, &agent_launch(Some("c1"), Some("/run/support")));
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
    let gemini = spawn_spec(&GEMINI, &agent_launch(Some("g1"), None));
    assert_eq!(value(&gemini, gemini::DEFAULTS_VARIABLE), None);
    let copilot = spawn_spec(&COPILOT, &agent_launch(Some("c1"), None));
    assert!(!copilot.args.iter().any(|arg| arg == "--plugin-dir"));
}

#[test]
fn the_prompt_a_report_carries_is_read_where_the_agent_sends_one() {
    let prompt =
        |harness: Harness, event: Option<&str>, body: &str| harness.prompt(event, body.as_bytes());
    assert_eq!(
        prompt(
            CLAUDE,
            None,
            r#"{"hook_event_name":"UserPromptSubmit","prompt":"Fix the login","session_id":"s"}"#
        )
        .as_deref(),
        Some("Fix the login")
    );
    // Not every report of Claude Code's is a prompt.
    assert_eq!(
        prompt(CLAUDE, None, r#"{"hook_event_name":"Stop","prompt":"no"}"#),
        None
    );
    assert_eq!(
        prompt(
            GEMINI,
            None,
            r#"{"hook_event_name":"BeforeAgent","prompt":"Add dark mode"}"#
        )
        .as_deref(),
        Some("Add dark mode")
    );
    assert_eq!(
        prompt(
            COPILOT,
            Some("userPromptSubmitted"),
            r#"{"prompt":"Why is CI slow?"}"#
        )
        .as_deref(),
        Some("Why is CI slow?")
    );
    assert_eq!(
        prompt(COPILOT, Some("sessionStart"), r#"{"prompt":"no"}"#),
        None
    );
    // Codex and OpenCode report no prompt; a shell reports nothing.
    for harness in [CODEX, OPENCODE, SHELL] {
        assert_eq!(prompt(harness, None, r#"{"prompt":"no"}"#), None);
    }
    assert_eq!(prompt(CLAUDE, None, "not json"), None);
}

/// An agent the person added, as their settings define it.
fn added(text: &str) -> Harness {
    let read =
        muster_core::config::preferences::MusterConfig::parse(Some(text), "settings.toml").unwrap();
    Harness::added(Arc::new(read.agents.into_iter().next().unwrap()))
}

const MY_AGENT: &str = r#"
[agents.my-agent]
label = "My Agent"
program = "my-agent"
args = ["--quiet"]
prompt = ["--message", "{prompt}"]
resume = ["--continue"]
env = { MY_AGENT_THEME = "dark", TERM = "xterm" }

[agents.my-agent.title]
working = "Thinking"
waiting = "Approve?"
"#;

#[test]
fn an_added_agent_is_found_after_muster_s_own() {
    let mine = added(MY_AGENT);
    let agents = [Arc::new(
        mine.added_agent()
            .cloned()
            .expect("an agent of the person's"),
    )];
    assert_eq!(find_with("my-agent", &agents), Some(mine.clone()));
    assert_eq!(find_with("claude", &agents), Some(CLAUDE));
    assert_eq!(find_with("other", &agents), None);
    // Muster's own are never the person's.
    assert_eq!(find("my-agent"), None);
    assert_eq!((mine.id(), mine.label()), ("my-agent", "My Agent"));
    for name in muster_core::config::agents::BUILT_IN {
        assert!(find(name).is_some(), "{name} is Muster's own");
    }
}

#[test]
fn an_added_agent_is_heard_without_hooks() {
    let mine = added(MY_AGENT);
    assert!(mine.is_agent() && mine.takes_prompt() && mine.can_resume());
    assert!(mine.resumes_without_id() && mine.suspends_itself());
    assert!(!mine.has_hooks() && !mine.chooses_session_id() && !mine.reports_start());
    assert_eq!(mine.hook_route(), None);
    assert_eq!(mine.translate(None, b"{}"), None);
    assert_eq!(mine.has_conversation("x"), None);
    // Its title, by the words its definition names.
    assert!(mine.title_reports_waits());
    assert_eq!(mine.read_title("⠋ Thinking"), Some(TitleState::Working));
    assert_eq!(mine.read_title("Approve? (y/n)"), Some(TitleState::Waiting));
    assert_eq!(mine.read_title("Ready"), None);

    let plain = added("[agents.plain]\nprogram = \"plain\"\n");
    assert!(plain.is_agent() && !plain.takes_prompt() && !plain.can_resume());
    assert!(!plain.resumes_without_id() && !plain.title_reports_waits());
    assert_eq!(plain.read_title("Thinking"), None);

    // One the settings no longer define, known by its name alone.
    let gone = Harness::added(Arc::new(AgentDefinition::bare("gone")));
    assert!(gone.is_agent() && !gone.takes_prompt() && !gone.can_resume());
    assert_eq!((gone.id(), gone.label()), ("gone", "gone"));
}

#[test]
fn an_added_agent_runs_as_its_settings_say() {
    let mine = added(MY_AGENT);
    let launch = |resume: bool| Launch {
        session: SessionId::new(),
        agent_session: resume.then(String::new),
        resume,
        prompt: Some("fix it".to_owned()),
        hooks: None,
        place: Place::Host(PathBuf::from("/work")),
        size: TermSize::default(),
        checks: None,
        support: None,
    };
    let fresh = spawn_spec(&mine, &launch(false));
    assert_eq!(fresh.program, "my-agent");
    assert_eq!(fresh.args, ["--quiet", "--message", "fix it"]);
    assert_eq!(fresh.cwd, PathBuf::from("/work"));
    assert_eq!(value(&fresh, "MY_AGENT_THEME"), Some("dark"));
    assert_eq!(value(&fresh, "MUSTER_HOOK_TOKEN"), None);
    // Its own values come after Muster's, so they win.
    let terms: Vec<&str> = fresh
        .env
        .iter()
        .filter(|(key, _)| key == "TERM")
        .map(|(_, value)| value.as_str())
        .collect();
    assert_eq!(terms, ["xterm-256color", "xterm"]);

    let resumed = spawn_spec(&mine, &launch(true));
    assert_eq!(resumed.args, ["--quiet", "--continue"]);
}

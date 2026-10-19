//! How a coding agent is asked one question and answers it on its
//! output, then ends (`PRD-03`): as data, so Muster's own agents and those
//! the person adds (`[agents.<name>] ask`, `model`, `models`) are asked
//! alike. Muster asks only to name a chat (`PRODUCT.md` §6), with the
//! person's own sign-in, and each way of asking here is one checked to
//! run none of their hooks, plugins or project instructions and to keep
//! nothing in the agent's history (`PRD-13`). Parsing and building do no
//! IO (`ARC-01`).

/// Where the question goes in [`Ask::args`].
pub const QUESTION: &str = "{prompt}";

/// Where the model goes in [`Ask::model`].
pub const MODEL: &str = "{model}";

/// How an agent is asked one question.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ask {
    /// The program, on `PATH` or by its path.
    pub program: String,
    /// The arguments that ask it, [`QUESTION`] in one of them.
    pub args: Vec<String>,
    /// The arguments that choose its model, [`MODEL`] in one of them, put
    /// before [`Self::args`]; `None` for an agent that has no choice.
    pub model: Option<Vec<String>>,
    /// The arguments that print its models, one to a line; `None` for an
    /// agent that cannot list them.
    pub models: Option<Vec<String>>,
    /// The model it is asked with unless the person picks one; `None` for
    /// its own default.
    pub default_model: Option<String>,
    /// Models offered where it lists none.
    pub suggestions: Vec<String>,
}

impl Ask {
    /// The arguments that ask it `question`, with `model` or its default.
    pub fn arguments(&self, question: &str, model: Option<&str>) -> Vec<String> {
        let mut args = Vec::new();
        if let (Some(template), Some(model)) =
            (&self.model, model.or(self.default_model.as_deref()))
        {
            args.extend(template.iter().map(|arg| arg.replace(MODEL, model)));
        }
        args.extend(self.args.iter().map(|arg| arg.replace(QUESTION, question)));
        args
    }
}

/// How Muster's own agents are asked one question, where each has been
/// checked to: Claude Code in safe mode, which loads none of the person's
/// hooks, plugins, skills, MCP servers or `CLAUDE.md` yet keeps their
/// sign-in, with no tools and nothing kept (checked against 2.1.303);
/// Codex's `exec`, run once, without the person's config or rules,
/// read-only (checked against 0.162.1). The others are asked once each is
/// checked the same way.
pub fn built_in(id: &str) -> Option<Ask> {
    let texts = |args: &[&str]| args.iter().map(|arg| (*arg).to_owned()).collect();
    match id {
        "claude" => Some(Ask {
            program: "claude".to_owned(),
            args: texts(&[
                "-p",
                "--safe-mode",
                "--tools",
                "",
                "--strict-mcp-config",
                "--no-session-persistence",
                QUESTION,
            ]),
            model: Some(texts(&["--model", MODEL])),
            models: None,
            default_model: Some("haiku".to_owned()),
            suggestions: texts(&["haiku", "sonnet", "opus"]),
        }),
        "codex" => Some(Ask {
            program: "codex".to_owned(),
            args: texts(&[
                "exec",
                "--ephemeral",
                "--skip-git-repo-check",
                "--ignore-user-config",
                "--ignore-rules",
                "--sandbox",
                "read-only",
                QUESTION,
            ]),
            model: Some(texts(&["--model", MODEL])),
            models: None,
            default_model: None,
            suggestions: Vec::new(),
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_code_is_asked_in_safe_mode_with_haiku_unless_another_is_picked() {
        let claude = built_in("claude").unwrap();
        let args = claude.arguments("Name it?", None);
        assert_eq!(&args[..2], ["--model", "haiku"]);
        for kept_out in [
            "--safe-mode",
            "--strict-mcp-config",
            "--no-session-persistence",
        ] {
            assert!(args.iter().any(|arg| arg == kept_out), "{kept_out}");
        }
        assert_eq!(args.last().map(String::as_str), Some("Name it?"));
        assert_eq!(
            &claude.arguments("q", Some("sonnet"))[..2],
            ["--model", "sonnet"]
        );
    }

    #[test]
    fn codex_is_asked_once_keeping_nothing_with_its_own_model_unless_picked() {
        let codex = built_in("codex").unwrap();
        let args = codex.arguments("Name it?", None);
        assert_eq!(args[0], "exec");
        assert!(args.iter().any(|arg| arg == "--ephemeral"));
        assert!(!args.iter().any(|arg| arg == "--model"));
        assert_eq!(&codex.arguments("q", Some("o4"))[..2], ["--model", "o4"]);
    }

    #[test]
    fn agents_not_checked_yet_are_not_asked() {
        for id in ["gemini", "opencode", "copilot", "shell", "checks"] {
            assert_eq!(built_in(id), None, "{id}");
        }
    }
}

//! The harnesses the person can start a session with, in the order the
//! new-session dialog offers them: the coding agents, then a shell.

/// A harness the new-session dialog offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HarnessChoice {
    Claude,
    Codex,
    Gemini,
    OpenCode,
    Copilot,
    Shell,
}

impl HarnessChoice {
    /// Every choice, in the order offered.
    pub(crate) const ALL: [Self; 6] = [
        Self::Claude,
        Self::Codex,
        Self::Gemini,
        Self::OpenCode,
        Self::Copilot,
        Self::Shell,
    ];

    /// The daemon's name for it.
    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Gemini => "gemini",
            Self::OpenCode => "opencode",
            Self::Copilot => "copilot",
            Self::Shell => "shell",
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::Codex => "Codex",
            Self::Gemini => "Gemini CLI",
            Self::OpenCode => "OpenCode",
            Self::Copilot => "Copilot CLI",
            Self::Shell => "Shell",
        }
    }

    /// The choice the daemon calls `id`, if it is one offered.
    pub(crate) fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|choice| choice.id() == id)
    }

    /// Whether it is a coding agent, which takes a first prompt; a shell
    /// takes none.
    pub(crate) fn is_agent(self) -> bool {
        self != Self::Shell
    }

    /// The next choice, round from the last to the first.
    pub(crate) fn next(self) -> Self {
        let at = Self::ALL
            .iter()
            .position(|choice| *choice == self)
            .unwrap_or(0);
        Self::ALL[(at + 1) % Self::ALL.len()]
    }

    /// The previous choice, round from the first to the last.
    pub(crate) fn previous(self) -> Self {
        let at = Self::ALL
            .iter()
            .position(|choice| *choice == self)
            .unwrap_or(0);
        Self::ALL[(at + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

/// Whether a session's harness suspends itself on Ctrl+Z, as every coding
/// agent does, and so would freeze with no shell above it to continue it;
/// with the name to say so by.
pub(crate) fn suspends_itself(harness: &str) -> Option<&'static str> {
    match harness {
        "review" => Some("Claude Code"),
        harness => HarnessChoice::from_id(harness)
            .filter(|choice| choice.is_agent())
            .map(HarnessChoice::label),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_choices_go_round_both_ways() {
        let mut choice = HarnessChoice::Claude;
        let mut seen = Vec::new();
        for _ in 0..HarnessChoice::ALL.len() {
            seen.push(choice);
            choice = choice.next();
        }
        assert_eq!(seen, HarnessChoice::ALL);
        assert_eq!(choice, HarnessChoice::Claude);
        assert_eq!(HarnessChoice::Claude.previous(), HarnessChoice::Shell);
        assert_eq!(HarnessChoice::Shell.previous(), HarnessChoice::Copilot);
    }

    #[test]
    fn each_choice_is_found_by_its_id() {
        for choice in HarnessChoice::ALL {
            assert_eq!(HarnessChoice::from_id(choice.id()), Some(choice));
        }
        assert_eq!(HarnessChoice::from_id("checks"), None);
    }

    #[test]
    fn every_agent_suspends_itself_and_a_shell_does_not() {
        assert_eq!(suspends_itself("codex"), Some("Codex"));
        assert_eq!(suspends_itself("review"), Some("Claude Code"));
        assert_eq!(suspends_itself("shell"), None);
        assert_eq!(suspends_itself("checks"), None);
    }
}

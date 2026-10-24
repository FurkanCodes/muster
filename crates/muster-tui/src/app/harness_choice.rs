//! The harnesses the person can start a session with, in the order the
//! dialogs offer them: Muster's own coding agents, then those the person
//! added in Muster's own settings (`[agents.<name>]`), then a shell.

use muster_core::harness::OwnAgent;
use muster_core::settings::AddedAgent;

/// A harness the dialogs offer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum HarnessChoice {
    /// One of Muster's own coding agents.
    Own(OwnAgent),
    Shell,
    /// A coding agent the person added in Muster's own settings.
    Added(AddedAgent),
}

impl HarnessChoice {
    /// Claude Code, offered first.
    pub(crate) const CLAUDE: Self = Self::Own(OwnAgent::Claude);

    /// Every choice, in the order offered: Muster's own agents, the
    /// person's (`added`), then a shell.
    pub(crate) fn all(added: &[AddedAgent]) -> Vec<Self> {
        OwnAgent::ALL
            .map(Self::Own)
            .into_iter()
            .chain(added.iter().cloned().map(Self::Added))
            .chain([Self::Shell])
            .collect()
    }

    /// The daemon's name for it.
    pub(crate) fn id(&self) -> &str {
        match self {
            Self::Own(agent) => agent.id(),
            Self::Shell => muster_core::harness::SHELL,
            Self::Added(agent) => &agent.name,
        }
    }

    pub(crate) fn label(&self) -> &str {
        match self {
            Self::Own(agent) => agent.label(),
            Self::Shell => "Shell",
            Self::Added(agent) => &agent.label,
        }
    }

    /// The choice the daemon calls `id`, if it is one offered.
    pub(crate) fn from_id(id: &str, added: &[AddedAgent]) -> Option<Self> {
        Self::all(added)
            .into_iter()
            .find(|choice| choice.id() == id)
    }

    /// Whether it is a coding agent; a shell is not.
    pub(crate) fn is_agent(&self) -> bool {
        !matches!(self, Self::Shell)
    }

    /// Whether it takes a first prompt: every agent of Muster's own, and
    /// one of the person's whose settings say how. A shell takes none.
    pub(crate) fn takes_prompt(&self) -> bool {
        match self {
            Self::Shell => false,
            Self::Added(agent) => agent.takes_prompt,
            Self::Own(_) => true,
        }
    }

    /// The choice `by` places on from this one, going round, among those
    /// `offered`; the first if this one is no longer there.
    fn step(&self, offered: &[Self], by: isize) -> Self {
        let Some(at) = offered.iter().position(|choice| choice == self) else {
            return offered.first().cloned().unwrap_or(Self::CLAUDE);
        };
        let len = offered.len().cast_signed();
        let next = (at.cast_signed() + by).rem_euclid(len);
        offered[next.cast_unsigned()].clone()
    }

    /// The next choice, round from the last to the first.
    pub(crate) fn next(&self, added: &[AddedAgent]) -> Self {
        self.step(&Self::all(added), 1)
    }

    /// The previous choice, round from the first to the last.
    pub(crate) fn previous(&self, added: &[AddedAgent]) -> Self {
        self.step(&Self::all(added), -1)
    }

    /// The agents a ticket or a tidy is offered, in order: those that take
    /// a first prompt, as theirs is handed to them.
    pub(crate) fn agents(added: &[AddedAgent]) -> Vec<Self> {
        Self::all(added)
            .into_iter()
            .filter(Self::takes_prompt)
            .collect()
    }

    /// The next agent that takes a prompt, passing over the shell, which a
    /// ticket is never given.
    pub(crate) fn next_agent(&self, added: &[AddedAgent]) -> Self {
        self.step(&Self::agents(added), 1)
    }

    /// The previous agent that takes a prompt, passing over the shell.
    pub(crate) fn previous_agent(&self, added: &[AddedAgent]) -> Self {
        self.step(&Self::agents(added), -1)
    }
}

/// Whether a session's harness suspends itself on Ctrl+Z, as every coding
/// agent does, and so would freeze with no shell above it to continue it;
/// with the name to say so by.
pub(crate) fn suspends_itself(harness: &str, added: &[AddedAgent]) -> Option<String> {
    match harness {
        muster_core::harness::REVIEW => Some(OwnAgent::Claude.label().to_owned()),
        harness => HarnessChoice::from_id(harness, added)
            .filter(HarnessChoice::is_agent)
            .map(|choice| choice.label().to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn added(name: &str, takes_prompt: bool) -> AddedAgent {
        AddedAgent {
            name: name.to_owned(),
            label: format!("{name} agent"),
            mark: "A".to_owned(),
            colour: None,
            takes_prompt,
            asks: false,
        }
    }

    #[test]
    fn the_choices_go_round_both_ways() {
        let mut choice = HarnessChoice::CLAUDE;
        let mut seen = Vec::new();
        for _ in 0..HarnessChoice::all(&[]).len() {
            seen.push(choice.clone());
            choice = choice.next(&[]);
        }
        assert_eq!(seen, HarnessChoice::all(&[]));
        assert_eq!(choice, HarnessChoice::CLAUDE);
        assert_eq!(HarnessChoice::CLAUDE.previous(&[]), HarnessChoice::Shell);
        assert_eq!(
            HarnessChoice::Shell.previous(&[]),
            HarnessChoice::Own(OwnAgent::Copilot)
        );
    }

    #[test]
    fn the_person_s_agents_come_after_muster_s_and_before_a_shell() {
        let mine = [added("my-agent", true), added("quiet", false)];
        let all = HarnessChoice::all(&mine);
        let ids: Vec<&str> = all.iter().map(HarnessChoice::id).collect();
        assert_eq!(
            ids,
            [
                "claude", "codex", "gemini", "opencode", "copilot", "my-agent", "quiet", "shell"
            ]
        );
        let mine_too = HarnessChoice::Added(mine[0].clone());
        assert_eq!(HarnessChoice::Own(OwnAgent::Copilot).next(&mine), mine_too);
        assert_eq!(mine_too.label(), "my-agent agent");
        assert!(mine_too.is_agent() && mine_too.takes_prompt());
        // One that takes no prompt is never offered a ticket.
        assert_eq!(mine_too.next_agent(&mine), HarnessChoice::CLAUDE);
        assert_eq!(HarnessChoice::CLAUDE.previous_agent(&mine), mine_too);
        // An agent no longer in the settings gives way to the first.
        let gone = HarnessChoice::Added(added("gone", true));
        assert_eq!(gone.next(&mine), HarnessChoice::CLAUDE);
        assert_eq!(
            HarnessChoice::from_id("quiet", &mine),
            Some(HarnessChoice::Added(mine[1].clone()))
        );
    }

    #[test]
    fn the_agents_go_round_without_the_shell() {
        let mut choice = HarnessChoice::CLAUDE;
        let mut seen = Vec::new();
        for _ in 0..5 {
            seen.push(choice.clone());
            choice = choice.next_agent(&[]);
        }
        assert_eq!(seen, HarnessChoice::agents(&[]));
        assert_eq!(choice, HarnessChoice::CLAUDE);
        assert_eq!(
            HarnessChoice::CLAUDE.previous_agent(&[]),
            HarnessChoice::Own(OwnAgent::Copilot)
        );
        assert_eq!(
            HarnessChoice::Own(OwnAgent::Codex).previous_agent(&[]),
            HarnessChoice::CLAUDE
        );
    }

    #[test]
    fn each_choice_is_found_by_its_id() {
        for choice in HarnessChoice::all(&[]) {
            assert_eq!(HarnessChoice::from_id(choice.id(), &[]), Some(choice));
        }
        assert_eq!(HarnessChoice::from_id("checks", &[]), None);
    }

    #[test]
    fn every_agent_suspends_itself_and_a_shell_does_not() {
        let mine = [added("my-agent", false)];
        assert_eq!(suspends_itself("codex", &[]).as_deref(), Some("Codex"));
        assert_eq!(
            suspends_itself("review", &[]).as_deref(),
            Some("Claude Code")
        );
        assert_eq!(
            suspends_itself("my-agent", &mine).as_deref(),
            Some("my-agent agent")
        );
        assert_eq!(suspends_itself("shell", &[]), None);
        assert_eq!(suspends_itself("checks", &[]), None);
    }
}

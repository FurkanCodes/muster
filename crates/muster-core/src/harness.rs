//! What a session is, as its harness string says (`SessionKind`), asked by
//! the daemon and the UI alike: `shell`, the person's own shell;
//! `plugin:<plugin>/<panel>`, a plugin's panel (`PRD-15`); `checks`, the
//! project's checks; anything else, a coding agent. The protocol and the
//! store carry the string; code reads it once into a [`SessionKind`] and
//! matches on that, so a new kind of session cannot slip past a rule. A
//! shell and a panel stand beside the chats rather than among them:
//! neither is ever news, counted among the agents or named as a chat, and
//! both end with their workspace. Pure (`ARC-01`).

/// The person's own shell in a workspace.
pub const SHELL: &str = "shell";

/// The project's checks, run by hand in a workspace.
pub const CHECKS: &str = "checks";

/// Claude Code reviewing a workspace's change, run by hand.
pub const REVIEW: &str = "review";

/// Whether `name` is one of Muster's own harnesses', which no agent of the
/// person's can take.
pub fn is_built_in(name: &str) -> bool {
    [SHELL, CHECKS, REVIEW].contains(&name) || OwnAgent::of(name).is_some()
}

/// What a plugin's panel's harness starts with.
const PANEL: &str = "plugin:";

/// The harness of `plugin`'s panel `panel`: `plugin:zoetrope/graph`.
pub fn panel(plugin: &str, panel: &str) -> String {
    format!("{PANEL}{plugin}/{panel}")
}

/// Muster's own coding agents, each in one place: the id its sessions and
/// settings name it by, how people know it, and how its program is found.
/// The daemon, the UIs and the command line all read them from here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "lowercase")]
pub enum OwnAgent {
    Claude,
    Codex,
    Gemini,
    OpenCode,
    Copilot,
}

impl OwnAgent {
    /// Every one, in the order Muster offers them.
    pub const ALL: [Self; 5] = [
        Self::Claude,
        Self::Codex,
        Self::Gemini,
        Self::OpenCode,
        Self::Copilot,
    ];

    /// The one whose id is `id`, if it is one.
    pub fn of(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|agent| agent.id() == id)
    }

    /// Its harness's id: what sessions and settings name it by, and its
    /// hooks' route.
    pub const fn id(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Gemini => "gemini",
            Self::OpenCode => "opencode",
            Self::Copilot => "copilot",
        }
    }

    /// Its name, as people know it.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::Codex => "Codex",
            Self::Gemini => "Gemini CLI",
            Self::OpenCode => "OpenCode",
            Self::Copilot => "Copilot CLI",
        }
    }

    /// The program it starts, before `PATH` is searched.
    pub const fn program(self) -> &'static str {
        self.id()
    }

    /// The names its program goes by in a terminal's foreground, after
    /// looking through whatever starts it (`front`).
    pub const fn program_names(self) -> &'static [&'static str] {
        match self {
            Self::Claude => &["claude", "claude-code"],
            Self::Codex => &["codex"],
            Self::Gemini => &["gemini"],
            Self::OpenCode => &["opencode"],
            Self::Copilot => &["copilot"],
        }
    }

    /// The packages it is installed from, as a package runner names them
    /// or a program or a runtime's script lies in them (`front`).
    pub const fn packages(self) -> &'static [&'static str] {
        match self {
            Self::Claude => &["@anthropic-ai/claude-code"],
            Self::Codex => &["@openai/codex"],
            Self::Gemini => &["@google/gemini-cli"],
            Self::OpenCode => &["opencode-ai", "@opencode-ai/cli"],
            Self::Copilot => &["@github/copilot"],
        }
    }

    /// The letter it is known by where only one fits.
    pub const fn letter(self) -> char {
        match self {
            Self::Claude => 'C',
            Self::Codex => 'X',
            Self::Gemini => 'G',
            Self::OpenCode => 'O',
            Self::Copilot => 'P',
        }
    }
}

/// What a session is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionKind<'a> {
    /// A coding agent, by its harness's id: a chat.
    Agent(&'a str),
    /// The project's checks.
    Checks,
    /// The person's own shell.
    Shell,
    /// A plugin's panel.
    Panel { plugin: &'a str, panel: &'a str },
}

impl<'a> SessionKind<'a> {
    /// What a session of `harness` is.
    pub fn of(harness: &'a str) -> Self {
        if harness == SHELL {
            return Self::Shell;
        }
        if harness == CHECKS {
            return Self::Checks;
        }
        let panel = harness
            .strip_prefix(PANEL)
            .and_then(|rest| rest.split_once('/'))
            .filter(|(plugin, panel)| {
                !plugin.is_empty() && !panel.is_empty() && !panel.contains('/')
            });
        match panel {
            Some((plugin, panel)) => Self::Panel { plugin, panel },
            None => Self::Agent(harness),
        }
    }

    /// Whether it is a coding agent's chat.
    pub const fn is_agent(self) -> bool {
        match self {
            Self::Agent(_) => true,
            Self::Checks | Self::Shell | Self::Panel { .. } => false,
        }
    }

    /// Whether it is the person's own shell.
    pub const fn is_shell(self) -> bool {
        match self {
            Self::Shell => true,
            Self::Agent(_) | Self::Checks | Self::Panel { .. } => false,
        }
    }

    /// Whether it is a plugin's panel.
    pub const fn is_panel(self) -> bool {
        match self {
            Self::Panel { .. } => true,
            Self::Agent(_) | Self::Checks | Self::Shell => false,
        }
    }

    /// Whether it stands beside the chats rather than among them: a shell
    /// or a panel, never news, never counted among the agents at work, and
    /// listed after its workspace's chats.
    pub const fn stands_beside(self) -> bool {
        match self {
            Self::Shell | Self::Panel { .. } => true,
            Self::Agent(_) | Self::Checks => false,
        }
    }

    /// Whether it ends with its workspace: removing its worktree stops it,
    /// where an agent still running refuses it.
    pub const fn ends_with_workspace(self) -> bool {
        match self {
            Self::Shell | Self::Panel { .. } => true,
            Self::Agent(_) | Self::Checks => false,
        }
    }

    /// Whether it is called by the program in its terminal's foreground
    /// (`zsh`, `cargo`): a shell, not a panel, whose title is its plugin's.
    pub const fn named_by_foreground(self) -> bool {
        match self {
            Self::Shell => true,
            Self::Agent(_) | Self::Checks | Self::Panel { .. } => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_of_muster_s_own_agents_is_read_back_by_its_id_and_serialized_as_it() {
        for agent in OwnAgent::ALL {
            assert_eq!(OwnAgent::of(agent.id()), Some(agent));
            assert!(is_built_in(agent.id()));
            let json = serde_json::to_string(&agent).unwrap();
            assert_eq!(json, format!("\"{}\"", agent.id()));
        }
        assert_eq!(OwnAgent::of("my-agent"), None);
        assert!(is_built_in("review") && is_built_in("shell") && !is_built_in("pi"));
    }

    #[test]
    fn a_panel_s_harness_names_its_plugin_and_panel() {
        let harness = panel("zoetrope", "graph");
        assert_eq!(harness, "plugin:zoetrope/graph");
        assert_eq!(
            SessionKind::of(&harness),
            SessionKind::Panel {
                plugin: "zoetrope",
                panel: "graph"
            }
        );
        for not in [
            "claude",
            "plugin:",
            "plugin:zoetrope",
            "plugin:/graph",
            "plugin:zoetrope/",
            "plugin:a/b/c",
            "my-plugin:a/b",
        ] {
            assert_eq!(SessionKind::of(not), SessionKind::Agent(not), "{not}");
        }
        assert_eq!(SessionKind::of("shell"), SessionKind::Shell);
        assert_eq!(SessionKind::of("checks"), SessionKind::Checks);
    }

    #[test]
    fn shells_and_panels_stand_beside_the_chats_and_end_with_their_workspace() {
        for (harness, beside, named, agent) in [
            ("shell", true, true, false),
            ("plugin:zoetrope/graph", true, false, false),
            ("claude", false, false, true),
            ("checks", false, false, false),
            ("my-agent", false, false, true),
        ] {
            let kind = SessionKind::of(harness);
            assert_eq!(kind.stands_beside(), beside, "{harness}");
            assert_eq!(kind.ends_with_workspace(), beside, "{harness}");
            assert_eq!(kind.named_by_foreground(), named, "{harness}");
            assert_eq!(kind.is_agent(), agent, "{harness}");
        }
    }
}

//! What a session's harness string says about it, asked by the daemon and
//! the UI alike: `shell`, the person's own shell; `plugin:<plugin>/<panel>`,
//! a plugin's panel (`PRD-15`); anything else, a coding agent or the
//! checks. A shell and a panel stand beside the chats rather than among
//! them: neither is ever news, counted among the agents or named as a
//! chat, and both end with their workspace. Pure (`ARC-01`).

/// The person's own shell in a workspace.
pub const SHELL: &str = "shell";

/// What a plugin's panel's harness starts with.
const PANEL: &str = "plugin:";

/// Whether `harness` is the person's own shell.
pub fn is_shell(harness: &str) -> bool {
    harness == SHELL
}

/// The harness of `plugin`'s panel `panel`: `plugin:zoetrope/graph`.
pub fn panel(plugin: &str, panel: &str) -> String {
    format!("{PANEL}{plugin}/{panel}")
}

/// The plugin and the panel `harness` names, if it is a plugin's panel.
pub fn panel_of(harness: &str) -> Option<(&str, &str)> {
    let (plugin, panel) = harness.strip_prefix(PANEL)?.split_once('/')?;
    (!plugin.is_empty() && !panel.is_empty() && !panel.contains('/')).then_some((plugin, panel))
}

/// Whether `harness` is a plugin's panel.
pub fn is_panel(harness: &str) -> bool {
    panel_of(harness).is_some()
}

/// Whether sessions of `harness` stand beside the chats rather than among
/// them: a shell or a panel, the person's own or a plugin's, never an
/// agent's. Such a session is never news, never counted among the agents
/// at work, and listed after its workspace's chats.
pub fn stands_beside(harness: &str) -> bool {
    is_shell(harness) || is_panel(harness)
}

/// Whether sessions of `harness` end with their workspace: removing its
/// worktree stops them, where an agent still running refuses it.
pub fn ends_with_workspace(harness: &str) -> bool {
    stands_beside(harness)
}

/// Whether sessions of `harness` are called by the program in their
/// terminal's foreground (`zsh`, `cargo`): a shell, not a panel, whose
/// title is its plugin's.
pub fn named_by_foreground(harness: &str) -> bool {
    is_shell(harness)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panel_s_harness_names_its_plugin_and_panel() {
        let harness = panel("zoetrope", "graph");
        assert_eq!(harness, "plugin:zoetrope/graph");
        assert_eq!(panel_of(&harness), Some(("zoetrope", "graph")));
        assert!(is_panel(&harness) && !is_shell(&harness));
        for not in [
            "shell",
            "claude",
            "plugin:",
            "plugin:zoetrope",
            "plugin:/graph",
            "plugin:zoetrope/",
            "plugin:a/b/c",
            "my-plugin:a/b",
        ] {
            assert_eq!(panel_of(not), None, "{not}");
        }
    }

    #[test]
    fn shells_and_panels_stand_beside_the_chats_and_end_with_their_workspace() {
        for (harness, beside, named) in [
            ("shell", true, true),
            ("plugin:zoetrope/graph", true, false),
            ("claude", false, false),
            ("checks", false, false),
            ("my-agent", false, false),
        ] {
            assert_eq!(stands_beside(harness), beside, "{harness}");
            assert_eq!(ends_with_workspace(harness), beside, "{harness}");
            assert_eq!(named_by_foreground(harness), named, "{harness}");
        }
    }
}

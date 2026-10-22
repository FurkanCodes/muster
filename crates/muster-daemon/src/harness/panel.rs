//! A plugin's panel (`PRD-15`, `ARC-23`): any terminal program a plugin
//! names, run in a session of its own beside the chats, as the person's
//! shell is, and told what Muster knows of the session or workspace it is
//! beside. It is no agent: no hooks, no resume, never news, and it ends
//! with its workspace (`muster_core::harness`). Its session is named
//! `plugin:<plugin>/<panel>`, so a restart in place carries it as any
//! session, and a daemon that crashed ends it, as a shell.

use muster_core::harness::SessionKind;

/// What runs for a plugin's panel: its program and arguments, its title,
/// and the variables it is given beyond every session's own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PanelLaunch {
    /// Its harness: `plugin:<plugin>/<panel>`.
    harness: String,
    /// Its session's title, as the plugin names the panel.
    pub(crate) title: String,
    /// The program, then its arguments; none for a panel carried through
    /// a restart in place, known by its harness alone.
    pub(crate) command: Vec<String>,
    /// Its variables, after the sessions' own, so a value it names wins:
    /// what Muster knows of where it is (`MUSTER_PLUGIN_*`).
    pub(crate) env: Vec<(String, String)>,
    /// The names of the person's variables its plugin may see, kept from
    /// the daemon's environment beside the basics every plugin gets
    /// (`scrub::kept_from_plugin`).
    pub(crate) seen: Vec<String>,
    /// The secrets its plugin names, each by its variable's name, as kept
    /// (`muster auth plugin`); never shown (`ARC-20`).
    pub(crate) secrets: Vec<(String, crate::secrets::Token)>,
}

impl PanelLaunch {
    /// `plugin`'s panel `panel`, titled `title`, running `command`.
    pub(crate) fn new(plugin: &str, panel: &str, title: &str, command: Vec<String>) -> Self {
        Self {
            harness: muster_core::harness::panel(plugin, panel),
            title: title.to_owned(),
            command,
            env: Vec::new(),
            seen: Vec::new(),
            secrets: Vec::new(),
        }
    }

    /// The panel that ran as `harness`, carried through a restart in
    /// place: its program runs on, and it is known by its harness alone,
    /// titled by its panel's id. `None` if `harness` is no panel's.
    pub(crate) fn carried(harness: &str) -> Option<Self> {
        let SessionKind::Panel { plugin, panel } = SessionKind::of(harness) else {
            return None;
        };
        Some(Self::new(plugin, panel, panel, Vec::new()))
    }

    /// Its harness: `plugin:<plugin>/<panel>`.
    pub(crate) fn harness(&self) -> &str {
        &self.harness
    }

    /// The variables it is given over the sessions' own: what Muster
    /// tells it, then its secrets.
    pub(crate) fn variables(&self) -> impl Iterator<Item = (String, String)> + '_ {
        let secrets = self
            .secrets
            .iter()
            .map(|(name, secret)| (name.clone(), secret.value().to_owned()));
        self.env.iter().cloned().chain(secrets)
    }

    /// Its program and that program's arguments.
    pub(crate) fn program(&self) -> (String, Vec<String>) {
        let mut command = self.command.iter().cloned();
        (command.next().unwrap_or_default(), command.collect())
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use muster_core::SessionId;
    use muster_core::protocol::TermSize;

    use super::super::{Harness, Launch, Place, carried, spawn_spec};
    use super::*;

    fn graph() -> Harness {
        let mut launch = PanelLaunch::new(
            "zoetrope",
            "graph",
            "graph",
            vec!["zoe".to_owned(), "--follow".to_owned()],
        );
        launch.env = vec![("MUSTER_PLUGIN_BRANCH".to_owned(), "fix".to_owned())];
        Harness::panel(launch)
    }

    #[test]
    fn a_panel_is_no_agent_and_is_named_by_its_plugin_and_panel() {
        let panel = graph();
        assert_eq!(panel.id(), "plugin:zoetrope/graph");
        assert_eq!(panel.label(), "graph");
        assert!(!panel.is_agent() && !panel.is_checks());
        assert!(!panel.has_hooks() && panel.hook_route().is_none());
        assert!(!panel.can_resume() && !panel.takes_prompt() && !panel.reports_start());
        assert!(!panel.chooses_session_id() && !panel.suspends_itself());
        assert!(panel.asking().is_none());
        assert_eq!(panel.translate(None, b"{}"), None);
    }

    #[test]
    fn a_panel_runs_its_program_where_it_is_told_with_its_variables() {
        let launch = Launch {
            session: SessionId::new(),
            agent_session: None,
            resume: false,
            prompt: None,
            hooks: None,
            place: Place::Host(PathBuf::from("/work/fix")),
            size: TermSize::default(),
            checks: None,
            support: None,
        };
        let spec = spawn_spec(&graph(), &launch);
        assert_eq!(
            (spec.program.as_str(), spec.args.as_slice()),
            ("zoe", &["--follow".to_owned()][..])
        );
        assert_eq!(spec.cwd, PathBuf::from("/work/fix"));
        let value = |key: &str| {
            spec.env
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value.as_str())
        };
        // A session's terminal, as every program's.
        assert_eq!(value("TERM_PROGRAM"), Some("muster"));
        assert_eq!(
            value("MUSTER_SESSION"),
            Some(launch.session.to_string().as_str())
        );
        assert_eq!(value("MUSTER_PLUGIN_BRANCH"), Some("fix"));
        assert_eq!(value("MUSTER_HOOK_TOKEN"), None, "no hooks");
    }

    #[test]
    fn a_carried_panel_is_taken_back_as_a_panel_not_an_agent() {
        let carried_panel = carried("plugin:zoetrope/graph", &[]);
        assert_eq!(carried_panel.id(), "plugin:zoetrope/graph");
        assert_eq!(carried_panel.label(), "graph");
        assert!(!carried_panel.is_agent() && !carried_panel.has_hooks());
        // An agent the settings no longer define is still known by name.
        let gone = carried("gone-agent", &[]);
        assert!(gone.is_agent() && !gone.can_resume());
        let shell = carried(
            "shell",
            &[Arc::new(
                muster_core::config::agents::AgentDefinition::bare("shell"),
            )],
        );
        assert!(!shell.is_agent(), "Muster's own first");
    }
}

//! What a panel's command is told: `{branch}`, `{transcript}` and the rest
//! in its arguments, filled in from what Muster knows of the session or
//! workspace it is beside. Pure: the daemon gathers the [`Context`].
//!
//! A placeholder is `{` and a lowercase word and `}`; other braces are the
//! argument's own (`{"x":1}`). A word that is no placeholder is refused as
//! the manifest is read, so filling in finds only known ones.

use std::fmt;

/// A value Muster fills in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Placeholder {
    /// The project's name.
    Project,
    /// The project's folder.
    Root,
    /// The workspace's folder.
    Workspace,
    /// What the workspace has checked out.
    Branch,
    /// The session's id.
    Session,
    /// The session's harness: `claude`, `codex`, an agent the person added.
    Harness,
    /// The id of the agent's conversation, as its resume takes it.
    Resume,
    /// The agent's own record of the conversation, Claude Code's or
    /// Codex's.
    Transcript,
    /// The ticket the workspace's branch is for.
    Ticket,
    /// The pull request from the workspace's branch, by its address.
    Pr,
}

impl Placeholder {
    /// Every placeholder, in the order the manual lists them.
    pub const ALL: [Self; 10] = [
        Self::Project,
        Self::Root,
        Self::Workspace,
        Self::Branch,
        Self::Session,
        Self::Harness,
        Self::Resume,
        Self::Transcript,
        Self::Ticket,
        Self::Pr,
    ];

    /// Its word, as written between braces.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Root => "root",
            Self::Workspace => "workspace",
            Self::Branch => "branch",
            Self::Session => "session",
            Self::Harness => "harness",
            Self::Resume => "resume",
            Self::Transcript => "transcript",
            Self::Ticket => "ticket",
            Self::Pr => "pr",
        }
    }

    /// The placeholder `name` writes, if it is one.
    pub fn named(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|placeholder| placeholder.name() == name)
    }

    /// Whether only a session has it, not a workspace by itself.
    pub const fn is_a_session_s(self) -> bool {
        matches!(
            self,
            Self::Session | Self::Harness | Self::Resume | Self::Transcript
        )
    }

    /// Whether a hook is told it: not an agent's conversation, as a hook
    /// is given names and ids only (`PRD-06`, `PRD-07`).
    pub const fn is_told_to_hooks(self) -> bool {
        !matches!(self, Self::Resume | Self::Transcript)
    }

    /// The variable a panel's process finds it in: `MUSTER_PLUGIN_BRANCH`.
    pub fn variable(self) -> String {
        format!("MUSTER_PLUGIN_{}", self.name().to_ascii_uppercase())
    }

    /// Why a context lacks it, for the person.
    const fn lacking(self) -> &'static str {
        match self {
            Self::Project | Self::Root => "it is in no project",
            Self::Workspace => "it is in no workspace",
            Self::Branch => "its workspace is on no branch",
            Self::Session | Self::Harness => "it is beside no session",
            Self::Resume => "its agent has no conversation to resume yet",
            Self::Transcript => "this session has no transcript yet",
            Self::Ticket => "its branch is for no ticket",
            Self::Pr => "its branch has no pull request",
        }
    }

    /// The words of `{name}`s in `argument`, in order: every `{` followed
    /// by lowercase letters and `}`, known or not.
    pub fn words_in(argument: &str) -> impl Iterator<Item = &str> {
        let mut rest = argument;
        std::iter::from_fn(move || {
            loop {
                let open = rest.find('{')?;
                let after = &rest[open + 1..];
                let length = after
                    .find(|c: char| !c.is_ascii_lowercase())
                    .unwrap_or(after.len());
                if length > 0 && after[length..].starts_with('}') {
                    rest = &after[length + 1..];
                    return Some(&after[..length]);
                }
                rest = after;
            }
        })
    }
}

impl fmt::Display for Placeholder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{{{}}}", self.name())
    }
}

/// What Muster knows of the session or workspace a panel is beside; a
/// value it does not know is `None`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Context {
    pub project: Option<String>,
    pub root: Option<String>,
    pub workspace: Option<String>,
    pub branch: Option<String>,
    pub session: Option<String>,
    pub harness: Option<String>,
    pub resume: Option<String>,
    pub transcript: Option<String>,
    pub ticket: Option<String>,
    pub pr: Option<String>,
}

impl Context {
    /// Its value for `placeholder`.
    pub fn get(&self, placeholder: Placeholder) -> Option<&str> {
        match placeholder {
            Placeholder::Project => self.project.as_deref(),
            Placeholder::Root => self.root.as_deref(),
            Placeholder::Workspace => self.workspace.as_deref(),
            Placeholder::Branch => self.branch.as_deref(),
            Placeholder::Session => self.session.as_deref(),
            Placeholder::Harness => self.harness.as_deref(),
            Placeholder::Resume => self.resume.as_deref(),
            Placeholder::Transcript => self.transcript.as_deref(),
            Placeholder::Ticket => self.ticket.as_deref(),
            Placeholder::Pr => self.pr.as_deref(),
        }
    }

    /// The `MUSTER_PLUGIN_*` variables of the values it has.
    pub fn variables(&self) -> Vec<(String, String)> {
        Placeholder::ALL
            .into_iter()
            .filter_map(|placeholder| {
                self.get(placeholder)
                    .map(|value| (placeholder.variable(), value.to_owned()))
            })
            .collect()
    }

    /// `arguments` with every placeholder filled in, or the first one this
    /// context has no value for, which refuses the panel: an argument is
    /// never left empty or half-filled.
    pub fn expand(&self, arguments: &[String]) -> Result<Vec<String>, Missing> {
        arguments
            .iter()
            .map(|argument| self.expand_one(argument))
            .collect()
    }

    fn expand_one(&self, argument: &str) -> Result<String, Missing> {
        let mut filled = String::with_capacity(argument.len());
        let mut rest = argument;
        while let Some(open) = rest.find('{') {
            let after = &rest[open + 1..];
            let length = after
                .find(|c: char| !c.is_ascii_lowercase())
                .unwrap_or(after.len());
            let word = &after[..length];
            match Placeholder::named(word) {
                Some(placeholder) if after[length..].starts_with('}') => {
                    let value = self.get(placeholder).ok_or(Missing { placeholder })?;
                    filled.push_str(&rest[..open]);
                    filled.push_str(value);
                    rest = &after[length + 1..];
                }
                _ => {
                    filled.push_str(&rest[..=open]);
                    rest = after;
                }
            }
        }
        filled.push_str(rest);
        Ok(filled)
    }
}

/// A placeholder a panel's command holds and its context has no value
/// for: the panel is refused, saying why.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{placeholder} has no value: {}", placeholder.lacking())]
pub struct Missing {
    pub placeholder: Placeholder,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|arg| (*arg).to_owned()).collect()
    }

    fn session() -> Context {
        Context {
            project: Some("app".to_owned()),
            root: Some("/src/app".to_owned()),
            workspace: Some("/w/app/fix".to_owned()),
            branch: Some("muster/issue-42-fix".to_owned()),
            session: Some("ses_1".to_owned()),
            harness: Some("claude".to_owned()),
            resume: Some("4b1d".to_owned()),
            transcript: Some("/c/projects/x/4b1d.jsonl".to_owned()),
            ticket: Some("42".to_owned()),
            pr: None,
        }
    }

    #[test]
    fn each_placeholder_is_filled_in_where_it_stands() {
        let filled = session()
            .expand(&args(&[
                "zoe",
                "{transcript}",
                "--on={branch}@{project}",
                "{root}/x",
                "{workspace}",
                "{session}:{harness}:{resume}:{ticket}",
            ]))
            .unwrap();
        assert_eq!(
            filled,
            args(&[
                "zoe",
                "/c/projects/x/4b1d.jsonl",
                "--on=muster/issue-42-fix@app",
                "/src/app/x",
                "/w/app/fix",
                "ses_1:claude:4b1d:42",
            ])
        );
    }

    #[test]
    fn other_braces_are_the_argument_s_own() {
        let kept = args(&["{\"x\":1}", "{}", "{Branch}", "a{b", "}{", "{branch"]);
        assert_eq!(session().expand(&kept).unwrap(), kept);
    }

    #[test]
    fn a_value_the_context_lacks_refuses_the_panel_with_why() {
        let missing = session().expand(&args(&["gh", "pr", "view", "{pr}"]));
        assert_eq!(
            missing,
            Err(Missing {
                placeholder: Placeholder::Pr
            })
        );
        let without = Context {
            transcript: None,
            ..session()
        };
        let error = without
            .expand(&args(&["zoe", "{transcript}"]))
            .unwrap_err()
            .to_string();
        assert_eq!(
            error,
            "{transcript} has no value: this session has no transcript yet"
        );
    }

    #[test]
    fn the_values_it_has_are_in_muster_plugin_variables_too() {
        let variables = session().variables();
        assert!(variables.contains(&(
            "MUSTER_PLUGIN_BRANCH".to_owned(),
            "muster/issue-42-fix".to_owned()
        )));
        assert!(variables.contains(&("MUSTER_PLUGIN_PROJECT".to_owned(), "app".to_owned())));
        assert!(!variables.iter().any(|(name, _)| name == "MUSTER_PLUGIN_PR"));
        assert_eq!(variables.len(), 9);
    }

    #[test]
    fn words_in_braces_are_found_known_or_not() {
        let found: Vec<&str> = Placeholder::words_in("a{branch}b{nope}{{x}{Pr}{}").collect();
        assert_eq!(found, ["branch", "nope", "x"]);
        for placeholder in Placeholder::ALL {
            assert_eq!(Placeholder::named(placeholder.name()), Some(placeholder));
            assert_eq!(
                placeholder.to_string(),
                format!("{{{}}}", placeholder.name())
            );
        }
        assert_eq!(Placeholder::named("nope"), None);
    }
}

//! Repositories on GitHub the person can clone as a project (`PRODUCT.md`
//! §6, §6a): listed, and cloned, by the daemon through the person's own
//! `gh`, as it is the daemon that talks to GitHub. From protocol 49
//! ([`REPOSITORIES_PROTOCOL`]). Pure (`ARC-01`).

use serde::{Deserialize, Serialize};

/// The first protocol whose daemon answers [`super::Command::ListRepositories`]
/// and [`super::Command::CloneProject`]; a client sends them only to one of
/// this or later.
pub const REPOSITORIES_PROTOCOL: u32 = 49;

/// A repository the person can reach on GitHub, as `gh` lists it: their
/// own, those they work on, and their organisations', the most recently
/// pushed first.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct HostedRepository {
    /// `owner/name`.
    pub name: String,
    pub description: Option<String>,
    pub private: bool,
}

/// `owner/name`, from what the person typed or chose: `owner/name` itself,
/// or a github.com address (`https://github.com/owner/name`, with `.git`
/// or without, or `git@github.com:owner/name.git`). `None` for anything
/// else, so `gh` is only ever given a repository's name, never an option
/// or a path.
pub fn repository_name(typed: &str) -> Option<String> {
    let typed = typed.trim();
    let bare = [
        "https://github.com/",
        "http://github.com/",
        "git@github.com:",
    ]
    .iter()
    .find_map(|prefix| typed.strip_prefix(prefix))
    .unwrap_or(typed);
    let bare = bare.trim_end_matches('/');
    let bare = bare.strip_suffix(".git").unwrap_or(bare);
    let (owner, name) = bare.split_once('/')?;
    let part = |part: &str| {
        !part.is_empty()
            && part.len() <= 100
            && !part.starts_with(['-', '.'])
            && part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    (part(owner) && part(name)).then(|| format!("{owner}/{name}"))
}

/// The folder a clone of `owner/name` goes in: its name.
pub fn folder_of(name: &str) -> &str {
    name.rsplit_once('/').map_or(name, |(_, folder)| folder)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_is_taken_as_typed_or_from_a_github_address() {
        for (typed, name) in [
            ("FurkanCodes/muster", "FurkanCodes/muster"),
            ("  o/app  ", "o/app"),
            ("https://github.com/o/app", "o/app"),
            ("https://github.com/o/app.git", "o/app"),
            ("https://github.com/o/app/", "o/app"),
            ("git@github.com:o/my_app.v2.git", "o/my_app.v2"),
        ] {
            assert_eq!(repository_name(typed).as_deref(), Some(name), "{typed}");
        }
    }

    #[test]
    fn anything_else_is_no_name() {
        for typed in [
            "",
            "app",
            "o/app/extra",
            "-o/app",
            "o/--upload-pack=x",
            "../app",
            "o/.hidden",
            "https://gitlab.com/o/app",
            "o/a p p",
            "/Users/you/app",
        ] {
            assert_eq!(repository_name(typed), None, "{typed}");
        }
    }

    #[test]
    fn a_clone_goes_in_a_folder_of_its_name() {
        assert_eq!(folder_of("o/app"), "app");
    }
}

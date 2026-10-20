//! Muster's own plugins, built into it (`ARCHITECTURE.md` §7a, *Ticket
//! plugins*): their manifests, compiled in and read as anyone's are, with
//! the ids kept for them. Their ticket sources run as `muster plugin-run
//! <id>`: the program `muster` in their commands is the daemon's own.
//!
//! What each declares in `[[tickets.settings]]` is what a project's
//! `[tickets]` may say for it, so `[tickets] tracker = "jira"` with its
//! `site` and `project` reads as it always has.

use std::sync::LazyLock;

use super::Manifest;

/// The program a command of Muster's own plugins names: Muster itself.
pub const PROGRAM: &str = "muster";

/// The plugin that comes with a git host: GitHub's issues.
pub const GITHUB: &str = "github";
/// GitLab's issues.
pub const GITLAB: &str = "gitlab";
/// Jira's tickets.
pub const JIRA: &str = "jira";
/// Linear's issues.
pub const LINEAR: &str = "linear";

/// The start of each of their manifests: its id, name and version, which
/// are Muster's.
macro_rules! head {
    ($id:literal, $name:literal, $description:literal) => {
        concat!(
            "id = \"",
            $id,
            "\"\nname = \"",
            $name,
            "\"\nversion = \"",
            env!("CARGO_PKG_VERSION"),
            "\"\nmuster = \"",
            env!("CARGO_PKG_VERSION"),
            "\"\ndescription = \"",
            $description,
            "\"\n"
        )
    };
}

/// Their manifests, as written.
pub const MANIFESTS: [&str; 4] = [
    concat!(
        head!(
            "github",
            "GitHub issues",
            "A repository's GitHub issues, through your own gh login."
        ),
        r#"requires = ["gh"]
how_to_get = "brew install gh, then gh auth login"

[tickets]
name = "GitHub"
noun = "issue"
command = ["muster", "plugin-run", "github"]
"#
    ),
    concat!(
        head!(
            "gitlab",
            "GitLab issues",
            "A repository's GitLab issues, through your own glab login."
        ),
        r#"requires = ["glab"]
how_to_get = "brew install glab, then glab auth login"

[tickets]
name = "GitLab"
noun = "issue"
command = ["muster", "plugin-run", "gitlab"]
"#
    ),
    concat!(
        head!(
            "jira",
            "Jira",
            "A Jira project's tickets, signed in with muster auth jira."
        ),
        r#"
[tickets]
name = "Jira"
noun = "ticket"
command = ["muster", "plugin-run", "jira"]
[[tickets.settings]]
key = "site"
label = "Jira site, such as acme.atlassian.net"
required = true
[[tickets.settings]]
key = "project"
label = "Project key, such as APP"
[[tickets.settings]]
key = "jql"
label = "Or a JQL query of your own"
"#
    ),
    concat!(
        head!(
            "linear",
            "Linear",
            "A Linear team's issues, signed in with muster auth linear."
        ),
        r#"
[tickets]
name = "Linear"
noun = "issue"
command = ["muster", "plugin-run", "linear"]
[[tickets.settings]]
key = "team"
label = "Team key, such as ENG"
required = true
"#
    ),
];

static READ: LazyLock<Vec<Manifest>> = LazyLock::new(|| {
    MANIFESTS
        .iter()
        .filter_map(|text| Manifest::parse_own(text).ok())
        .collect()
});

/// Their manifests, as read. A test proves each reads.
pub fn manifests() -> &'static [Manifest] {
    &READ
}

/// The manifest of Muster's own plugin `id`, if it has one.
pub fn manifest(id: &str) -> Option<&'static Manifest> {
    manifests().iter().find(|manifest| manifest.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_of_muster_s_own_manifests_reads_with_its_kept_id() {
        assert_eq!(manifests().len(), MANIFESTS.len());
        // Anyone else's manifest may not take their ids.
        assert!(MANIFESTS.iter().all(|text| Manifest::parse(text).is_err()));
        for id in [GITHUB, GITLAB, JIRA, LINEAR] {
            let manifest = manifest(id).unwrap();
            assert!(super::super::is_reserved(id));
            let tickets = manifest.tickets.as_ref().unwrap();
            assert_eq!(tickets.command, [PROGRAM, "plugin-run", id]);
            assert_eq!(manifest.version, env!("CARGO_PKG_VERSION"));
        }
    }

    #[test]
    fn they_declare_what_projects_say_for_them_today() {
        let keys = |id: &str| -> Vec<(String, bool)> {
            manifest(id)
                .unwrap()
                .tickets
                .as_ref()
                .unwrap()
                .settings
                .iter()
                .map(|setting| (setting.key.clone(), setting.required))
                .collect()
        };
        assert_eq!(keys(GITHUB), []);
        assert_eq!(keys(GITLAB), []);
        assert_eq!(
            keys(JIRA),
            [
                ("site".to_owned(), true),
                ("project".to_owned(), false),
                ("jql".to_owned(), false)
            ]
        );
        assert_eq!(keys(LINEAR), [("team".to_owned(), true)]);
        let named = |id: &str| {
            let tickets = manifest(id).unwrap().tickets.clone().unwrap();
            format!("{} {}", tickets.name, tickets.noun)
        };
        assert_eq!(named(GITHUB), "GitHub issue");
        assert_eq!(named(GITLAB), "GitLab issue");
        assert_eq!(named(JIRA), "Jira ticket");
        assert_eq!(named(LINEAR), "Linear issue");
    }
}

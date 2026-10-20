use super::*;

/// A manifest with every key, as the plan's example writes it.
const FULL: &str = r#"
id = "zoetrope"
name = "zoetrope"
version = "0.1.0"
description = "Watch a session as a live flow graph."
muster = "0.3.0"
requires = ["zoe"]
how_to_get = "brew install someone/tap/zoetrope, or cargo install zoetrope"
env = ["ZOE_THEME"]
secrets = ["ZOE_TOKEN"]

[[panels]]
id = "graph"
title = "graph"
on = "session"
command = ["zoe", "{transcript}"]
key = "Z"

[[panels]]
id = "tree"
on = "workspace"
command = ["./bin/tree", "--root={workspace}", "--branch", "{branch}"]

[tickets]
name = "YouTrack"
noun = "issue"
command = ["./youtrack", "--serve"]
[[tickets.settings]]
key = "site"
label = "Site"
required = true
[[tickets.settings]]
key = "project"
"#;

fn read(text: &str) -> Result<Manifest, ManifestError> {
    super::read(text, "0.3.35", false)
}

fn error(text: &str) -> String {
    read(text).unwrap_err().to_string()
}

/// `FULL` with `old` replaced by `new`.
fn changed(old: &str, new: &str) -> String {
    assert!(FULL.contains(old), "{old}");
    FULL.replacen(old, new, 1)
}

#[test]
fn every_key_of_a_manifest_reads() {
    let manifest = read(FULL).unwrap();
    assert_eq!(manifest.id, "zoetrope");
    assert_eq!(manifest.name, "zoetrope");
    assert_eq!(manifest.version, "0.1.0");
    assert_eq!(
        manifest.description.as_deref(),
        Some("Watch a session as a live flow graph.")
    );
    assert_eq!(manifest.muster, "0.3.0");
    assert_eq!(manifest.requires, ["zoe"]);
    assert!(manifest.how_to_get.unwrap().starts_with("brew install"));
    assert_eq!(manifest.env, ["ZOE_THEME"]);
    assert_eq!(manifest.secrets, ["ZOE_TOKEN"]);
    assert_eq!(
        manifest.panels,
        [
            Panel {
                id: "graph".to_owned(),
                title: "graph".to_owned(),
                on: PanelOn::Session,
                command: vec!["zoe".to_owned(), "{transcript}".to_owned()],
                key: Some("Z".to_owned()),
            },
            Panel {
                id: "tree".to_owned(),
                // The id, where no title is written.
                title: "tree".to_owned(),
                on: PanelOn::Workspace,
                command: ["./bin/tree", "--root={workspace}", "--branch", "{branch}"]
                    .map(str::to_owned)
                    .to_vec(),
                key: None,
            },
        ]
    );
    assert_eq!(
        manifest.tickets,
        Some(TicketSource {
            name: "YouTrack".to_owned(),
            noun: "issue".to_owned(),
            command: vec!["./youtrack".to_owned(), "--serve".to_owned()],
            settings: vec![
                TicketSetting {
                    key: "site".to_owned(),
                    label: "Site".to_owned(),
                    required: true,
                },
                TicketSetting {
                    key: "project".to_owned(),
                    label: "project".to_owned(),
                    required: false,
                },
            ],
        })
    );
}

#[test]
fn every_command_it_would_run_is_listed_with_where_it_is_written() {
    let manifest = read(FULL).unwrap();
    let commands: Vec<(String, Vec<String>)> = manifest
        .commands()
        .into_iter()
        .map(|(at, command)| (at, command.to_vec()))
        .collect();
    assert_eq!(commands.len(), 3);
    assert_eq!(commands[0].0, "panels[0].command");
    assert_eq!(commands[1].0, "panels[1].command");
    assert_eq!(
        commands[2],
        (
            "tickets.command".to_owned(),
            vec!["./youtrack".to_owned(), "--serve".to_owned()]
        )
    );
}

#[test]
fn the_least_a_manifest_holds_is_who_it_is_and_one_thing_it_gives() {
    let least = "id = \"p\"\nname = \"P\"\nversion = \"1\"\nmuster = \"0.3\"\n\n\
                 [[panels]]\nid = \"x\"\ncommand = [\"x\"]\n";
    let manifest = read(least).unwrap();
    assert_eq!(manifest.panels[0].on, PanelOn::Session);
    assert_eq!(manifest.tickets, None);
    assert_eq!(manifest.requires, Vec::<String>::new());
    let nothing = "id = \"p\"\nname = \"P\"\nversion = \"1\"\nmuster = \"0.3\"\n";
    assert_eq!(read(nothing), Err(ManifestError::Nothing));
    for key in ["id", "name", "version", "muster"] {
        let without = least
            .lines()
            .filter(|line| !line.starts_with(&format!("{key} =")))
            .collect::<Vec<_>>()
            .join("\n");
        let error = error(&without);
        assert!(error.starts_with(&format!("{key} is missing")), "{error}");
    }
}

#[test]
fn an_unknown_key_is_refused_naming_where_it_is() {
    for (text, says) in [
        (
            changed("requires", "require"),
            "require is not a key of a plugin's manifest",
        ),
        (
            changed("key = \"Z\"", "keys = \"Z\""),
            "panels[0].keys is not a key of a plugin's manifest here: they are id, title, on, \
             command, key",
        ),
        (
            changed("noun = \"issue\"", "noun = \"issue\"\nicon = \"y\""),
            "tickets.icon",
        ),
        (
            changed("label = \"Site\"", "caption = \"Site\""),
            "tickets.settings[0].caption",
        ),
    ] {
        let error = error(&text);
        assert!(error.starts_with(says), "{error}");
    }
}

#[test]
fn a_value_of_the_wrong_kind_is_refused_naming_its_key() {
    for (text, key) in [
        (
            changed("version = \"0.1.0\"", "version = 1"),
            "version must be",
        ),
        (
            changed("version = \"0.1.0\"", "version = \"one two\""),
            "version must be",
        ),
        (
            changed("requires = [\"zoe\"]", "requires = \"zoe\""),
            "requires must be",
        ),
        (
            changed("requires = [\"zoe\"]", "requires = [\"/usr/bin/zoe\"]"),
            "requires must be",
        ),
        (
            changed("env = [\"ZOE_THEME\"]", "env = [\"MUSTER_SESSION\"]"),
            "env must be",
        ),
        (
            changed("secrets = [\"ZOE_TOKEN\"]", "secrets = [\"1TOKEN\"]"),
            "secrets must be",
        ),
        (
            changed("on = \"session\"", "on = \"tab\""),
            "panels[0].on must be",
        ),
        (
            changed("key = \"Z\"", "key = \"hyper-z\""),
            "panels[0].key must be",
        ),
        (
            changed("command = [\"zoe\", \"{transcript}\"]", "command = \"zoe\""),
            "panels[0].command must be",
        ),
        (
            changed("command = [\"zoe\", \"{transcript}\"]", "command = []"),
            "panels[0].command must be",
        ),
        (
            changed("required = true", "required = \"yes\""),
            "tickets.settings[0].required must be",
        ),
        (
            changed("noun = \"issue\"", "noun = \"issue-7\""),
            "tickets.noun must be",
        ),
        (
            changed("muster = \"0.3.0\"", "muster = \"latest\""),
            "muster must be",
        ),
        (
            changed("name = \"zoetrope\"", "name = \"line\\nbreak\""),
            "name must be",
        ),
        (
            changed(
                "description = \"Watch a session as a live flow graph.\"",
                "description = \"\"",
            ),
            "description must be",
        ),
    ] {
        let error = error(&text);
        assert!(error.starts_with(key), "{key}: {error}");
    }
    assert!(error("id = [").starts_with("is not valid TOML"));
}

#[test]
fn a_command_s_program_is_on_path_by_its_full_path_or_inside_the_plugin() {
    for good in ["zoe", "./zoe", "./bin/zoe", "/usr/local/bin/zoe"] {
        let text = changed("[\"zoe\", \"{transcript}\"]", &format!("[\"{good}\"]"));
        assert!(read(&text).is_ok(), "{good}");
    }
    for bad in [
        "../zoe",
        "./../zoe",
        "./",
        "bin/zoe",
        "-zoe",
        "",
        "/a/../zoe",
        "{root}/zoe",
    ] {
        let text = changed("[\"zoe\", \"{transcript}\"]", &format!("[\"{bad}\"]"));
        let error = error(&text);
        assert!(
            error.starts_with("panels[0].command must be"),
            "{bad}: {error}"
        );
    }
}

#[test]
fn ids_must_be_short_lowercase_and_not_muster_s_own() {
    let error_of = |id: &str| error(&changed("id = \"zoetrope\"", &format!("id = \"{id}\"")));
    assert!(error_of("Zoetrope").starts_with("id must be 1 to 32"));
    assert!(error_of("-zoe").starts_with("id must be"));
    for own in [
        "github",
        "jira",
        "linear",
        "gitlab",
        "muster",
        "muster-tickets",
    ] {
        assert_eq!(
            error_of(own),
            format!("id: {own:?} is kept for Muster's own plugins; call it something else")
        );
    }
    // Muster's own read with theirs.
    let own = changed("id = \"zoetrope\"", "id = \"github\"");
    assert_eq!(super::read(&own, "0.3.35", true).unwrap().id, "github");
    let panel = changed("id = \"graph\"", "id = \"Graph\"");
    assert!(error(&panel).starts_with("panels[0].id must be"));
    let twice = changed("id = \"tree\"", "id = \"graph\"");
    assert_eq!(error(&twice), "panels[1].id: \"graph\" is another's too");
}

#[test]
fn a_plugin_needing_a_newer_muster_is_refused_saying_which() {
    let newer = changed("muster = \"0.3.0\"", "muster = \"0.4.0\"");
    assert_eq!(
        super::read(&newer, "0.3.35", false),
        Err(ManifestError::TooNew {
            needs: "0.4.0".to_owned(),
            running: "0.3.35".to_owned()
        })
    );
    assert_eq!(
        error(&newer),
        "muster: it needs Muster 0.4.0 or later, and this is 0.3.35"
    );
    assert!(super::read(&newer, "0.4.0", false).is_ok());
    assert!(super::read(&newer, "0.10.1", false).is_ok());
}

#[test]
fn a_word_in_braces_that_is_no_placeholder_is_refused() {
    let typo = changed("{transcript}", "{transcrpt}");
    let error = error(&typo);
    assert!(
        error
            .starts_with("panels[0].command: {transcrpt} is not a placeholder: they are {project}"),
        "{error}"
    );
    // A workspace's panel has no session to tell of.
    let session = changed("\"--branch\", \"{branch}\"", "\"--chat\", \"{session}\"");
    assert_eq!(
        self::error(&session),
        "panels[1].command: {session} is a session's, and this panel is a workspace's"
    );
    // Braces of the argument's own are kept as they are.
    let json = changed("\"zoe\", \"{transcript}\"", "\"zoe\", \"{\\\"x\\\":1}\"");
    assert!(read(&json).is_ok());
    // A ticket source serves every project, so it is told none.
    let ticket = changed(
        "\"./youtrack\", \"--serve\"",
        "\"./youtrack\", \"{project}\"",
    );
    assert_eq!(read(&ticket), Err(ManifestError::TicketPlaceholder));
}

#[test]
fn ticket_settings_keep_muster_s_own_keys_and_name_each_once() {
    let tracker = changed("key = \"site\"", "key = \"tracker\"");
    assert!(error(&tracker).starts_with("tickets.settings[0].key must be"));
    let twice = changed("key = \"project\"", "key = \"site\"");
    assert_eq!(
        error(&twice),
        "tickets.settings[1].key: \"site\" is another's too"
    );
    let not_a_table = "id = \"p\"\nname = \"P\"\nversion = \"1\"\nmuster = \"0.3\"\ntickets = 3\n";
    assert!(error(not_a_table).starts_with("tickets must be a [tickets] table"));
}

#[test]
fn the_muster_reading_it_knows_its_own_version() {
    // This build reads a manifest made for it.
    let made = changed(
        "muster = \"0.3.0\"",
        &format!("muster = \"{}\"", crate::plugins::MUSTER_VERSION),
    );
    assert!(Manifest::parse(&made).is_ok());
}

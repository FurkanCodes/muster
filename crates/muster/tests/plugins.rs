//! End-to-end tests of plugins (`PRD-15`, `ARC-23`, `ARCHITECTURE.md` §7a)
//! against the real daemon: `muster plugin` installs one from a `file://`
//! git repository the test makes, after showing everything it would run
//! and asking; links a folder; turns one off and on; updates it to a new
//! commit; removes it with its files. A manifest edited after its approval
//! turns its plugin off with why, until it is approved again. Nothing of a
//! plugin is run. The examples in `examples/plugins` read as manifests.

// Helpers outside `#[test]` functions are not covered by clippy.toml's test exemption.
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "end-to-end tests: a failed step is the test failing"
)]
#![expect(
    clippy::disallowed_methods,
    reason = "tests time themselves and start their own threads and tasks; ARC-03, ARC-13 and ARC-14 bind Muster's own code"
)]

mod support;

use support::wait;

use std::os::unix::fs::MetadataExt as _;
use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};

use muster_core::protocol::plugins::PluginProblem;
use muster_core::protocol::{Delta, ServerFrame};
use serde_json::Value;
use sha2::Digest as _;

use support::{Sandbox, expect_event, git};

const MANIFEST: &str = r#"id = "zoe"
name = "Zoe"
version = "0.1.0"
description = "Watch a session as a graph."
muster = "0.3.0"
requires = ["sh", "zoe-not-on-path"]
how_to_get = "cargo install zoe"
env = ["ZOE_THEME"]
secrets = ["ZOE_TOKEN"]

[[panels]]
id = "graph"
title = "graph"
command = ["./graph", "{branch}", "{transcript}"]
key = "Z"

[tickets]
name = "Zoe tickets"
noun = "task"
command = ["./tickets", "--serve"]
[[tickets.settings]]
key = "site"
label = "Site"
required = true
"#;

/// A git repository holding a plugin with `manifest`, and a program of
/// its that would leave a mark if anything ran it.
fn plugin_repository(sandbox: &Sandbox, manifest: &str) -> PathBuf {
    let root = sandbox.repository("zoe-plugin");
    std::fs::write(root.join("muster-plugin.toml"), manifest).unwrap();
    let ran = sandbox.root().join("ran");
    std::fs::write(
        root.join("graph"),
        format!("#!/bin/sh\ntouch '{}'\n", ran.display()),
    )
    .unwrap();
    git(&root, &["add", "muster-plugin.toml", "graph"]);
    git(&root, &["commit", "--quiet", "--message", "the plugin"]);
    root
}

/// `muster plugin <args>` with no terminal, `env` set for it.
fn plugin(sandbox: &Sandbox, args: &[&str], env: &[(&str, &str)]) -> Output {
    sandbox
        .command()
        .arg("plugin")
        .args(args)
        .envs(env.iter().copied())
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

/// `muster plugin <args>`, which must succeed; what it printed.
fn ok(sandbox: &Sandbox, args: &[&str]) -> String {
    let output = plugin(sandbox, args, &[]);
    assert!(
        output.status.success(),
        "muster plugin {args:?}: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// `muster plugin <args>`, which must fail; what it said.
fn refused(sandbox: &Sandbox, args: &[&str], env: &[(&str, &str)]) -> String {
    let output = plugin(sandbox, args, env);
    assert!(
        !output.status.success(),
        "muster plugin {args:?} succeeded: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// `muster plugin list --json`'s plugins, but Muster's own, which are
/// always there.
fn listed(sandbox: &Sandbox) -> Vec<Value> {
    let document: Value = serde_json::from_str(&ok(sandbox, &["list", "--json"])).unwrap();
    assert_eq!(document["format"], 2);
    let plugins = document["plugins"].as_array().unwrap();
    let own: Vec<&str> = plugins
        .iter()
        .filter(|plugin| plugin["source"] == "builtin")
        .map(|plugin| plugin["id"].as_str().unwrap())
        .collect();
    assert_eq!(own, ["github", "gitlab", "jira", "linear"]);
    plugins
        .iter()
        .filter(|plugin| plugin["source"] != "builtin")
        .cloned()
        .collect()
}

fn the_one(sandbox: &Sandbox) -> Value {
    let plugins = listed(sandbox);
    assert_eq!(plugins.len(), 1, "{plugins:?}");
    plugins[0].clone()
}

fn settings(sandbox: &Sandbox) -> String {
    std::fs::read_to_string(sandbox.paths().config_dir.join("settings.toml")).unwrap_or_default()
}

fn plugins_dir(sandbox: &Sandbox) -> PathBuf {
    sandbox.paths().data_dir.join("plugins")
}

fn sha256(text: &str) -> String {
    muster_core::hex::encode(&sha2::Sha256::digest(text.as_bytes()))
}

/// Waits up to ten seconds for `done`: a folder is removed off the
/// daemon's loop, a moment after the command that let it go.
fn wait_until(what: &str, done: impl Fn() -> bool) {
    wait::until(what, done);
}

fn address(repository: &Path) -> String {
    format!("file://{}", repository.display())
}

#[test]
fn installing_shows_everything_it_would_run_and_asks_first_prd_15() {
    let sandbox = Sandbox::new();
    let _daemon = sandbox.start();
    let repository = plugin_repository(&sandbox, MANIFEST);
    let url = address(&repository);

    // No terminal to ask on: refused, nothing kept.
    let error = refused(&sandbox, &["install", &url], &[]);
    assert!(
        error.contains("there is no terminal to ask you on"),
        "{error}"
    );
    assert!(error.contains("--yes"), "{error}");
    // Inside a Muster session, refused even with --yes.
    let error = refused(
        &sandbox,
        &["install", &url, "--yes"],
        &[("MUSTER_SESSION", "ses_01M5CT6R6VGYWCD90DMW085ZXY")],
    );
    assert!(error.contains("inside a Muster session"), "{error}");
    assert!(error.contains("Settings screen"), "{error}");
    assert_eq!(listed(&sandbox), Vec::<Value>::new());
    assert!(!settings(&sandbox).contains("plugins"));

    let shown = ok(&sandbox, &["install", &url, "--yes"]);
    for says in [
        "Zoe 0.1.0 (zoe), from file://",
        "Watch a session as a graph.",
        "works with Muster 0.3.0 or later",
        "panel graph (beside a session, key Z) runs: ./graph {branch} {transcript}",
        "ticket source Zoe tickets runs: ./tickets --serve",
        "a project's [tickets] holds site (Site, required)",
        "needs on PATH: sh (found), zoe-not-on-path (not found)",
        "to get them: cargo install zoe",
        "sees your variables: ZOE_THEME",
        "is given the secrets: ZOE_TOKEN",
        &format!("its manifest's SHA-256: {}", sha256(MANIFEST)),
        "Install zoe and turn it on? Yes, as --yes says.",
        "Installed zoe and turned it on.",
    ] {
        assert!(shown.contains(says), "{says:?} in:\n{shown}");
    }

    let zoe = the_one(&sandbox);
    assert_eq!(zoe["id"], "zoe");
    assert_eq!(zoe["source"], "git");
    assert_eq!(zoe["url"], url.as_str());
    assert_eq!(zoe["runs"], true);
    assert_eq!(zoe["problem"], Value::Null);
    assert_eq!(zoe["manifest_sha256"], sha256(MANIFEST).as_str());
    assert_eq!(zoe["panels"][0]["command"][0], "./graph");
    assert_eq!(zoe["tickets"]["settings"][0]["key"], "site");
    assert_eq!(zoe["requires"][1]["found"], false);
    assert_eq!(zoe["changed"]["what"], "installed");
    assert_eq!(zoe["changed"]["by"], "command_line");
    let commit = zoe["commit"].as_str().unwrap().to_owned();
    assert_eq!(commit, git(&repository, &["rev-parse", "HEAD"]));

    // In Muster's own folder, private to the person; its entry in
    // Muster's own settings, with what was approved (PRD-13).
    let installed = plugins_dir(&sandbox).join("zoe").join(&commit);
    assert!(installed.join("muster-plugin.toml").is_file());
    for folder in [
        plugins_dir(&sandbox),
        plugins_dir(&sandbox).join("zoe"),
        installed,
    ] {
        assert_eq!(
            std::fs::metadata(&folder).unwrap().mode() & 0o777,
            0o700,
            "{}",
            folder.display()
        );
    }
    let written: toml::Table = settings(&sandbox).parse().unwrap();
    let entry = &written["plugins"]["zoe"];
    assert_eq!(entry["git"].as_str(), Some(url.as_str()));
    assert_eq!(entry["commit"].as_str(), Some(commit.as_str()));
    assert_eq!(entry["enabled"].as_bool(), Some(true));
    assert_eq!(entry["approved"].as_str(), Some(sha256(MANIFEST).as_str()));
    assert!(
        !plugins_dir(&sandbox)
            .join(".incoming")
            .read_dir()
            .unwrap()
            .any(|_| true)
    );
    // Nothing of it ran.
    assert!(!sandbox.root().join("ran").exists());

    let list = ok(&sandbox, &["list"]);
    // After Muster's own.
    assert!(list.starts_with("github "), "{list}");
    assert!(list.contains("\nzoe 0.1.0: on, from file://"), "{list}");
    assert!(list.contains(&format!("at {}", &commit[..7])), "{list}");
    assert!(
        list.contains("installed 0 s ago from the command line"),
        "{list}"
    );
}

#[tokio::test]
async fn every_client_is_told_of_a_change_to_the_plugins_arc_09() {
    let sandbox = Sandbox::new();
    let daemon = sandbox.start();
    let repository = plugin_repository(&sandbox, MANIFEST);
    let mut connection = daemon.connect().await;
    let snapshot = connection.subscribe().await.unwrap();
    let theirs = |plugins: &[muster_core::protocol::plugins::PluginInfo]| {
        plugins
            .iter()
            .filter(|plugin| plugin.source != muster_core::plugins::PluginSource::Builtin)
            .cloned()
            .collect::<Vec<_>>()
    };
    assert_eq!(theirs(&snapshot.plugins), []);
    assert_eq!(snapshot.plugins.len(), 4, "Muster's own");
    ok(&sandbox, &["install", &address(&repository), "--yes"]);
    let plugins = expect_event(&mut connection, |frame| match frame {
        ServerFrame::Delta(Delta::PluginsChanged(plugins)) => Some(plugins.clone()),
        _ => None,
    })
    .await;
    let added = theirs(&plugins);
    assert_eq!(added.len(), 1);
    assert!(added[0].runs());
    let mut again = daemon.connect().await;
    assert_eq!(again.subscribe().await.unwrap().plugins, plugins);
}

#[tokio::test]
async fn the_person_s_settings_read_again_read_the_plugins_again_prd_15() {
    let sandbox = Sandbox::new();
    let daemon = sandbox.start();
    let folder = sandbox.root().join("my-zoe");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("muster-plugin.toml"), MANIFEST).unwrap();
    let path = folder.canonicalize().unwrap().display().to_string();
    ok(&sandbox, &["link", &path, "--yes"]);
    let mut connection = daemon.connect().await;
    let zoe = |plugins: &[muster_core::protocol::plugins::PluginInfo]| {
        plugins
            .iter()
            .find(|plugin| plugin.id == "zoe")
            .cloned()
            .unwrap()
    };
    assert!(zoe(&connection.subscribe().await.unwrap().plugins).runs());

    std::fs::write(
        folder.join("muster-plugin.toml"),
        format!("{MANIFEST}# more\n"),
    )
    .unwrap();
    let reply = connection
        .command(muster_core::protocol::Command::ReadPreferences)
        .await
        .unwrap();
    assert_eq!(reply, muster_core::protocol::Reply::Done);
    let plugins = expect_event(&mut connection, |frame| match frame {
        ServerFrame::Delta(Delta::PluginsChanged(plugins)) => Some(plugins.clone()),
        _ => None,
    })
    .await;
    assert_eq!(zoe(&plugins).problem, Some(PluginProblem::Changed));
}

#[test]
fn a_plugin_is_turned_off_and_on_and_removed_with_its_files() {
    let sandbox = Sandbox::new();
    let _daemon = sandbox.start();
    let repository = plugin_repository(&sandbox, MANIFEST);
    ok(&sandbox, &["install", &address(&repository), "--yes"]);

    assert_eq!(ok(&sandbox, &["disable", "zoe"]), "Turned zoe off.\n");
    let zoe = the_one(&sandbox);
    assert_eq!(
        (zoe["enabled"].clone(), zoe["runs"].clone()),
        (false.into(), false.into())
    );
    assert_eq!(zoe["changed"]["what"], "turned_off");
    assert!(settings(&sandbox).contains("enabled = false"));

    // Turning on asks too, so not without a terminal, nor in a session.
    let error = refused(&sandbox, &["enable", "zoe"], &[]);
    assert!(error.contains("no terminal to ask you on"), "{error}");
    let error = refused(
        &sandbox,
        &["enable", "zoe", "--yes"],
        &[("MUSTER_SESSION", "ses_x")],
    );
    assert!(error.contains("inside a Muster session"), "{error}");
    assert_eq!(the_one(&sandbox)["enabled"], false);
    let shown = ok(&sandbox, &["enable", "zoe", "--yes"]);
    assert!(
        shown.contains("panel graph (beside a session, key Z) runs: ./graph"),
        "{shown}"
    );
    assert!(shown.ends_with("Turned zoe on.\n"), "{shown}");
    assert_eq!(the_one(&sandbox)["runs"], true);
    assert_eq!(
        ok(&sandbox, &["enable", "zoe", "--yes"]),
        "zoe is on already.\n"
    );

    let error = refused(&sandbox, &["remove", "zoe"], &[]);
    assert!(error.contains("no terminal to ask you on"), "{error}");
    let error = refused(&sandbox, &["remove", "nope", "--yes"], &[]);
    assert!(
        error.contains(
            "there is no plugin called nope; there are github, gitlab, jira, linear, zoe"
        ),
        "{error}"
    );
    let shown = ok(&sandbox, &["remove", "zoe", "--yes"]);
    assert!(shown.ends_with("Removed zoe.\n"), "{shown}");
    assert_eq!(listed(&sandbox), Vec::<Value>::new());
    assert!(
        !settings(&sandbox).contains("zoe"),
        "{}",
        settings(&sandbox)
    );
    // The rest of the file stays as it was, its first lines too.
    assert!(
        settings(&sandbox).starts_with("# Your own settings for Muster as a whole."),
        "{}",
        settings(&sandbox)
    );
    wait_until("its folder to go", || {
        !plugins_dir(&sandbox).join("zoe").exists()
    });
    // Muster's own stay.
    let list = ok(&sandbox, &["list"]);
    assert!(!list.contains("zoe"), "{list}");
    assert!(list.starts_with("github "), "{list}");
}

#[test]
fn an_update_fetches_the_new_commit_and_lets_the_old_one_go() {
    let sandbox = Sandbox::new();
    let _daemon = sandbox.start();
    let repository = plugin_repository(&sandbox, MANIFEST);
    ok(
        &sandbox,
        &["install", &address(&repository), "--ref", "main", "--yes"],
    );
    let old = the_one(&sandbox)["commit"].as_str().unwrap().to_owned();
    assert_eq!(the_one(&sandbox)["ref"], "main");

    let newer = MANIFEST.replace("0.1.0", "0.2.0");
    std::fs::write(repository.join("muster-plugin.toml"), &newer).unwrap();
    git(
        &repository,
        &["commit", "--quiet", "--all", "--message", "0.2.0"],
    );
    let error = refused(&sandbox, &["update", "zoe"], &[]);
    assert!(error.contains("no terminal to ask you on"), "{error}");
    let shown = ok(&sandbox, &["update", "zoe", "--yes"]);
    assert!(
        shown.starts_with("Zoe 0.2.0 (zoe), from file://"),
        "{shown}"
    );
    assert!(
        shown.contains(&format!(
            "now 0.1.0 from file://{} at {}",
            repository.display(),
            &old[..7]
        )),
        "{shown}"
    );
    assert!(
        shown.contains("with a manifest that is not this one"),
        "{shown}"
    );
    assert!(shown.ends_with("Updated zoe to 0.2.0.\n"), "{shown}");

    let zoe = the_one(&sandbox);
    let new = zoe["commit"].as_str().unwrap().to_owned();
    assert_eq!(new, git(&repository, &["rev-parse", "HEAD"]));
    assert_eq!(zoe["version"], "0.2.0");
    assert_eq!(zoe["runs"], true);
    assert_eq!(zoe["changed"]["what"], "updated");
    assert!(plugins_dir(&sandbox).join("zoe").join(&new).is_dir());
    wait_until("the old commit to go", || {
        !plugins_dir(&sandbox).join("zoe").join(&old).exists()
    });
    assert!(settings(&sandbox).contains(&sha256(&newer)));
}

#[test]
fn a_manifest_edited_after_approval_turns_the_plugin_off_with_why_prd_15_arc_23() {
    let sandbox = Sandbox::new();
    let mut daemon = sandbox.start();
    let repository = plugin_repository(&sandbox, MANIFEST);
    ok(&sandbox, &["install", &address(&repository), "--yes"]);
    let commit = the_one(&sandbox)["commit"].as_str().unwrap().to_owned();
    let copy = plugins_dir(&sandbox)
        .join("zoe")
        .join(&commit)
        .join("muster-plugin.toml");

    // A command added to it after its preview.
    let edited = MANIFEST.replace(
        "[\"./tickets\", \"--serve\"]",
        "[\"./tickets\", \"--upload\"]",
    );
    std::fs::write(&copy, &edited).unwrap();
    let zoe = the_one(&sandbox);
    assert_eq!(zoe["enabled"], true);
    assert_eq!(zoe["runs"], false);
    assert_eq!(zoe["problem"], "changed");
    assert_eq!(
        zoe["problem_message"],
        PluginProblem::Changed.to_string().as_str()
    );
    // What it would run now is shown.
    assert_eq!(zoe["tickets"]["command"][1], "--upload");
    let list = ok(&sandbox, &["list"]);
    assert!(list.contains("zoe 0.1.0: on, not running"), "{list}");
    assert!(
        list.contains("its manifest changed since you approved it"),
        "{list}"
    );
    let error = refused(&sandbox, &["enable", "zoe", "--yes"], &[]);
    assert!(
        error.contains("zoe cannot be turned on: its manifest changed"),
        "{error}"
    );

    // Read again as the daemon starts, it is still off.
    drop(daemon);
    daemon = sandbox.start();
    assert_eq!(the_one(&sandbox)["problem"], "changed");

    // Updated, it is approved again from its repository, and on.
    let shown = ok(&sandbox, &["update", "zoe", "--yes"]);
    assert!(
        shown.contains("with a manifest that is not this one"),
        "{shown}"
    );
    let zoe = the_one(&sandbox);
    assert_eq!(zoe["runs"], true, "{zoe}");
    assert_eq!(zoe["tickets"]["command"][1], "--serve");
    assert_eq!(std::fs::read_to_string(&copy).unwrap(), MANIFEST);
    drop(daemon);
}

#[test]
fn a_linked_folder_is_approved_again_by_linking_it_again_and_left_where_it_is() {
    let sandbox = Sandbox::new();
    let _daemon = sandbox.start();
    let folder = sandbox.root().join("my-zoe");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("muster-plugin.toml"), MANIFEST).unwrap();
    let path = folder.canonicalize().unwrap().display().to_string();

    let error = refused(&sandbox, &["link", &path], &[]);
    assert!(error.contains("no terminal to ask you on"), "{error}");
    let error = refused(
        &sandbox,
        &["link", &path, "--yes"],
        &[("MUSTER_SESSION", "ses_x")],
    );
    assert!(error.contains("inside a Muster session"), "{error}");
    let shown = ok(&sandbox, &["link", &path, "--yes"]);
    assert!(
        shown.contains(&format!("Zoe 0.1.0 (zoe), linked from {path}")),
        "{shown}"
    );
    assert!(shown.ends_with("Linked zoe and turned it on.\n"), "{shown}");
    let zoe = the_one(&sandbox);
    assert_eq!(
        (zoe["source"].clone(), zoe["folder"].clone()),
        ("folder".into(), path.clone().into())
    );
    assert_eq!(zoe["runs"], true);
    // Muster keeps no copy of a linked folder.
    assert!(!plugins_dir(&sandbox).join("zoe").exists());

    let edited = format!("{MANIFEST}\n[[panels]]\nid = \"more\"\ncommand = [\"./more\"]\n");
    std::fs::write(folder.join("muster-plugin.toml"), &edited).unwrap();
    assert_eq!(the_one(&sandbox)["problem"], "changed");
    let shown = ok(&sandbox, &["link", &path, "--yes"]);
    assert!(
        shown.contains("panel more (beside a session) runs: ./more"),
        "{shown}"
    );
    assert!(
        shown.ends_with("Approved zoe as its folder holds it now.\n"),
        "{shown}"
    );
    assert_eq!(the_one(&sandbox)["runs"], true);
    // `update` reads a linked folder again too.
    std::fs::write(folder.join("muster-plugin.toml"), MANIFEST).unwrap();
    assert!(
        ok(&sandbox, &["update", "zoe", "--yes"])
            .ends_with("Approved zoe as its folder holds it now.\n")
    );

    let shown = ok(&sandbox, &["remove", "zoe", "--yes"]);
    assert!(
        shown.contains(&format!("Its folder, {path}, stays where it is.")),
        "{shown}"
    );
    assert_eq!(listed(&sandbox), Vec::<Value>::new());
    assert_eq!(
        std::fs::read_to_string(folder.join("muster-plugin.toml")).unwrap(),
        MANIFEST
    );
}

#[test]
fn what_cannot_be_installed_is_refused_with_why() {
    let sandbox = Sandbox::new();
    let _daemon = sandbox.start();
    let error = refused(
        &sandbox,
        &["install", "http://example.com/zoe.git", "--yes"],
        &[],
    );
    assert!(error.contains("http:// is refused"), "{error}");
    let error = refused(
        &sandbox,
        &["install", "https://me:hunter2@example.com/zoe.git", "--yes"],
        &[],
    );
    assert!(error.contains("holds a password"), "{error}");
    assert!(!error.contains("hunter2"), "{error}");

    let not_a_plugin = sandbox.repository("not-a-plugin");
    let error = refused(
        &sandbox,
        &["install", &address(&not_a_plugin), "--yes"],
        &[],
    );
    assert!(error.contains("has no muster-plugin.toml"), "{error}");
    let own = MANIFEST.replace("id = \"zoe\"", "id = \"github\"");
    let repository = plugin_repository(&sandbox, &own);
    let error = refused(&sandbox, &["install", &address(&repository), "--yes"], &[]);
    assert!(
        error.contains("\"github\" is kept for Muster's own plugins"),
        "{error}"
    );
    let error = refused(
        &sandbox,
        &[
            "install",
            &address(&repository),
            "--ref=--upload-pack=x",
            "--yes",
        ],
        &[],
    );
    assert!(
        error.contains("\"--upload-pack=x\" is not a branch or tag"),
        "{error}"
    );
    assert_eq!(listed(&sandbox), Vec::<Value>::new());
    // Every clone it fetched was let go.
    wait_until("the clones to go", || {
        plugins_dir(&sandbox)
            .join(".incoming")
            .read_dir()
            .map_or(true, |mut listing| listing.next().is_none())
    });
}

#[test]
fn a_plugin_installed_twice_is_refused_the_second_time() {
    let sandbox = Sandbox::new();
    let _daemon = sandbox.start();
    let repository = plugin_repository(&sandbox, MANIFEST);
    // A repository's folder, as well as its address.
    ok(
        &sandbox,
        &["install", &repository.display().to_string(), "--yes"],
    );
    let error = refused(&sandbox, &["install", &address(&repository), "--yes"], &[]);
    assert!(
        error.contains("a plugin called zoe is installed already"),
        "{error}"
    );
    assert!(error.contains("muster plugin update zoe"), "{error}");
    assert_eq!(listed(&sandbox).len(), 1);
}

#[test]
fn every_example_plugin_s_manifest_reads_prd_15() {
    // docs/plugins.md points its readers at these: each must link as it is.
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/plugins");
    let mut read = Vec::new();
    for entry in std::fs::read_dir(&examples).unwrap() {
        let folder = entry.unwrap().path();
        let text = std::fs::read_to_string(folder.join("muster-plugin.toml")).unwrap();
        let manifest = muster_core::plugins::Manifest::parse(&text)
            .unwrap_or_else(|error| panic!("{}: {error}", folder.display()));
        assert_eq!(
            folder.file_name().unwrap().to_str().unwrap(),
            manifest.id,
            "an example's folder is named for its plugin"
        );
        assert!(
            folder.join("README.md").is_file(),
            "{} has no README",
            folder.display()
        );
        read.push(manifest.id);
    }
    read.sort();
    assert_eq!(
        read,
        ["notify-webhook", "session-graph", "tickets-from-file"]
    );
}

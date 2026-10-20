use std::path::{Path, PathBuf};

use muster_core::protocol::plugins::{ChangedBy, PluginAction, PluginProblem, PreviewKind};
use muster_core::time::Timestamp;

use super::*;
use crate::git::tests::{git, repository};

const MANIFEST: &str = "id = \"zoe\"\nname = \"zoe\"\nversion = \"0.1.0\"\nmuster = \"0.3\"\n\
                        requires = [\"sh\", \"no-such-program-here\"]\n\n[[panels]]\n\
                        id = \"graph\"\ncommand = [\"./zoe\", \"{branch}\"]\n";

/// A daemon's folders: its data, its settings, and room for plugins.
struct Folders {
    dir: tempfile::TempDir,
}

impl Folders {
    fn new() -> Self {
        Self {
            dir: tempfile::tempdir().unwrap(),
        }
    }

    fn data(&self) -> PathBuf {
        self.dir.path().join("data")
    }

    fn config(&self) -> PathBuf {
        self.dir.path().join("config")
    }

    fn registry(&self) -> Registry {
        let mut registry = Registry::new(&self.data(), self.config());
        let reading = registry.read();
        registry.take(reading);
        registry
    }

    fn settings(&self) -> String {
        std::fs::read_to_string(self.config().join("settings.toml")).unwrap_or_default()
    }

    /// A folder holding a plugin, `manifest` its manifest.
    fn plugin_folder(&self, name: &str, manifest: &str) -> PathBuf {
        let folder = self.dir.path().join(name);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("muster-plugin.toml"), manifest).unwrap();
        folder.canonicalize().unwrap()
    }

    /// A git repository holding a plugin, `manifest` its manifest.
    fn plugin_repository(&self, manifest: &str) -> PathBuf {
        let root = repository(self.dir.path(), "zoe-repo");
        std::fs::write(root.join("muster-plugin.toml"), manifest).unwrap();
        git(&root, &["add", "muster-plugin.toml"]);
        git(&root, &["commit", "--quiet", "--message", "manifest"]);
        root
    }
}

fn at() -> Timestamp {
    Timestamp::from_millis(1_760_000_000_000)
}

fn now() -> std::time::Instant {
    crate::clock::monotonic()
}

/// Fetches and holds a preview, as `Preview` does.
async fn preview(
    registry: &mut Registry,
    job: Job,
) -> muster_core::protocol::plugins::PluginPreview {
    let found = fetch(job, registry.incoming()).await.unwrap();
    let (preview, gone) = registry.hold(found, now());
    assert_eq!(gone, Vec::<PathBuf>::new());
    preview.unwrap()
}

fn reread(registry: &mut Registry) {
    let reading = registry.read();
    registry.take(reading);
}

fn only(registry: &Registry) -> PluginInfo {
    let infos = registry.infos();
    assert_eq!(infos.len(), 1, "{infos:?}");
    infos.into_iter().next().unwrap()
}

#[tokio::test]
async fn a_changed_manifest_turns_its_plugin_off_until_approved_again_prd_15_arc_23() {
    let folders = Folders::new();
    let folder = folders.plugin_folder("zoe", MANIFEST);
    let mut registry = folders.registry();
    let job = Job::Folder {
        path: folder.clone(),
        update_of: None,
    };
    let shown = preview(&mut registry, job).await;
    assert_eq!(shown.kind, PreviewKind::Link);
    assert_eq!(shown.details.sha256, sha256(MANIFEST.as_bytes()));
    let (linked, gone) = registry.install(&shown.preview, ChangedBy::CommandLine, now(), at());
    linked.unwrap();
    assert_eq!(gone, Vec::<PathBuf>::new());
    reread(&mut registry);
    let info = only(&registry);
    assert!(info.runs(), "{info:?}");
    assert_eq!(
        info.changed.map(|change| (change.what, change.by)),
        Some((PluginAction::Linked, ChangedBy::CommandLine))
    );

    // A command added to it after its preview.
    let edited = MANIFEST.replace("[\"./zoe\", \"{branch}\"]", "[\"./zoe\", \"--upload\"]");
    std::fs::write(folder.join("muster-plugin.toml"), &edited).unwrap();
    reread(&mut registry);
    let info = only(&registry);
    assert_eq!(info.problem, Some(PluginProblem::Changed));
    assert!(info.enabled && !info.runs());
    // What it would run now is shown, to approve or not.
    assert_eq!(
        info.details.unwrap().panels[0].command,
        ["./zoe", "--upload"]
    );
    let refused = registry.set_enabled("zoe", true, ChangedBy::Ui, at());
    assert!(
        matches!(
            &refused,
            Err(PluginError::CannotRun {
                problem: PluginProblem::Changed,
                ..
            })
        ),
        "{refused:?}"
    );
    assert!(
        refused
            .unwrap_err()
            .to_string()
            .contains("changed since you approved it")
    );

    // Linked again, it is approved again, and runs.
    let job = registry.update_job("zoe").unwrap();
    let again = preview(&mut registry, job).await;
    assert_eq!(again.kind, PreviewKind::Link);
    assert_eq!(
        again
            .current
            .as_ref()
            .map(|current| current.problem.clone()),
        Some(Some(PluginProblem::Changed))
    );
    registry
        .install(&again.preview, ChangedBy::Ui, now(), at())
        .0
        .unwrap();
    reread(&mut registry);
    assert!(only(&registry).runs());
    assert!(folders.settings().contains(&sha256(edited.as_bytes())));
}

#[tokio::test]
async fn removal_leaves_nothing_keyed_by_the_plugin_arc_14() {
    let folders = Folders::new();
    let repository = folders.plugin_repository(MANIFEST);
    let mut registry = folders.registry();
    let job = Job::Clone {
        url: repository.display().to_string(),
        git_ref: None,
        update_of: None,
    };
    let shown = preview(&mut registry, job).await;
    assert_eq!(shown.kind, PreviewKind::Install);
    let PluginSource::Git { commit, .. } = &shown.source else {
        panic!("{:?}", shown.source);
    };
    let installed = folders.data().join("plugins/zoe").join(commit);
    registry
        .install(&shown.preview, ChangedBy::Ui, now(), at())
        .0
        .unwrap();
    reread(&mut registry);
    assert!(installed.join("muster-plugin.toml").is_file());
    assert!(only(&registry).runs());
    assert!(folders.settings().contains("[plugins.zoe]"));
    // Which programs it needs are found, and which not.
    let required: Vec<bool> = only(&registry)
        .details
        .unwrap()
        .requires
        .iter()
        .map(|required| required.found)
        .collect();
    assert_eq!(required, [true, false]);

    // Another preview of it waits, as the person looks again.
    let update = registry.update_job("zoe").unwrap();
    let waiting = preview(&mut registry, update).await;
    assert_eq!(waiting.kind, PreviewKind::Update);
    assert_eq!(registry.previews(), 1);

    let (removed, gone) = registry.remove("zoe");
    removed.unwrap();
    for folder in gone {
        files::remove_later(folder).await;
    }
    reread(&mut registry);
    assert_eq!(registry.infos(), []);
    assert_eq!(registry.previews(), 0);
    assert!(registry.changes.is_empty());
    assert!(!folders.data().join("plugins/zoe").exists());
    assert!(
        !folders.settings().contains("zoe"),
        "{}",
        folders.settings()
    );
    let incoming = folders.data().join("plugins/.incoming");
    assert_eq!(std::fs::read_dir(&incoming).unwrap().count(), 0);
    assert!(
        registry
            .install(&waiting.preview, ChangedBy::Ui, now(), at())
            .0
            .is_err()
    );
}

#[tokio::test]
async fn an_update_moves_to_the_new_commit_and_lets_the_old_one_go() {
    let folders = Folders::new();
    let repository = folders.plugin_repository(MANIFEST);
    let mut registry = folders.registry();
    let job = Job::Clone {
        url: repository.display().to_string(),
        git_ref: None,
        update_of: None,
    };
    let shown = preview(&mut registry, job).await;
    registry
        .install(&shown.preview, ChangedBy::Ui, now(), at())
        .0
        .unwrap();
    reread(&mut registry);
    registry
        .set_enabled("zoe", false, ChangedBy::Ui, at())
        .unwrap();
    reread(&mut registry);
    let PluginSource::Git { commit: old, .. } = only(&registry).source else {
        panic!();
    };
    std::fs::write(
        repository.join("muster-plugin.toml"),
        MANIFEST.replace("0.1.0", "0.2.0"),
    )
    .unwrap();
    git(
        &repository,
        &["commit", "--quiet", "--all", "--message", "0.2.0"],
    );

    let job = registry.update_job("zoe").unwrap();
    let update = preview(&mut registry, job).await;
    assert_eq!(update.details.version, "0.2.0");
    assert_eq!(
        update
            .current
            .as_ref()
            .unwrap()
            .details
            .as_ref()
            .unwrap()
            .version,
        "0.1.0"
    );
    let (updated, gone) = registry.install(&update.preview, ChangedBy::CommandLine, now(), at());
    updated.unwrap();
    assert_eq!(gone, [folders.data().join("plugins/zoe").join(&old)]);
    reread(&mut registry);
    let info = only(&registry);
    let PluginSource::Git { commit: new, .. } = &info.source else {
        panic!();
    };
    assert_ne!(*new, old);
    assert!(folders.data().join("plugins/zoe").join(new).is_dir());
    // Updated, it stays off as the person had it.
    assert!(!info.enabled && info.problem.is_none());
    assert_eq!(info.changed.unwrap().what, PluginAction::Updated);
}

#[tokio::test]
async fn an_edited_copy_is_replaced_by_the_same_commit_approved_again() {
    let folders = Folders::new();
    let repository = folders.plugin_repository(MANIFEST);
    let mut registry = folders.registry();
    let job = Job::Clone {
        url: format!("file://{}", repository.display()),
        git_ref: Some("main".to_owned()),
        update_of: None,
    };
    let shown = preview(&mut registry, job).await;
    registry
        .install(&shown.preview, ChangedBy::Ui, now(), at())
        .0
        .unwrap();
    reread(&mut registry);
    let PluginSource::Git {
        commit, git_ref, ..
    } = only(&registry).source
    else {
        panic!();
    };
    assert_eq!(git_ref.as_deref(), Some("main"));
    let copy = folders.data().join("plugins/zoe").join(&commit);
    std::fs::write(
        copy.join("muster-plugin.toml"),
        format!("{MANIFEST}\n# edited\n"),
    )
    .unwrap();
    reread(&mut registry);
    assert_eq!(only(&registry).problem, Some(PluginProblem::Changed));

    let job = registry.update_job("zoe").unwrap();
    let again = preview(&mut registry, job).await;
    let (updated, gone) = registry.install(&again.preview, ChangedBy::Ui, now(), at());
    updated.unwrap();
    assert_eq!(gone.len(), 1, "the edited copy, set aside");
    reread(&mut registry);
    assert!(only(&registry).runs());
    assert_eq!(
        std::fs::read_to_string(copy.join("muster-plugin.toml")).unwrap(),
        MANIFEST
    );
}

#[tokio::test]
async fn a_taken_id_or_a_renamed_update_is_refused_and_its_clone_let_go() {
    let folders = Folders::new();
    let repository = folders.plugin_repository(MANIFEST);
    let mut registry = folders.registry();
    let clone = || Job::Clone {
        url: repository.display().to_string(),
        git_ref: None,
        update_of: None,
    };
    let shown = preview(&mut registry, clone()).await;
    registry
        .install(&shown.preview, ChangedBy::Ui, now(), at())
        .0
        .unwrap();
    reread(&mut registry);

    let found = fetch(clone(), registry.incoming()).await.unwrap();
    let held = found.clone.clone().unwrap();
    let (refused, gone) = registry.hold(found, now());
    assert!(
        matches!(refused, Err(PluginError::Installed { .. })),
        "{refused:?}"
    );
    assert_eq!(gone, [held]);

    let linked = folders.plugin_folder("other", &MANIFEST.replace("\"zoe\"", "\"other\""));
    let job = Job::Folder {
        path: linked,
        update_of: Some("zoe".to_owned()),
    };
    let found = fetch(job, registry.incoming()).await.unwrap();
    let (refused, _) = registry.hold(found, now());
    assert!(
        refused
            .unwrap_err()
            .to_string()
            .starts_with("its manifest calls it other now, not zoe"),
    );
    assert!(matches!(
        registry.remove("nope").0,
        Err(PluginError::NoPlugin { .. })
    ));
}

#[tokio::test]
async fn what_cannot_be_a_plugin_is_refused_before_anything_is_kept() {
    let folders = Folders::new();
    let incoming = folders.data().join("plugins/.incoming");
    for (job, says) in [
        (
            Job::Clone {
                url: "http://example.com/zoe".to_owned(),
                git_ref: None,
                update_of: None,
            },
            "http:// is refused",
        ),
        (
            Job::Clone {
                url: "/no/such/repository".to_owned(),
                git_ref: None,
                update_of: None,
            },
            "could not fetch the plugin",
        ),
        (
            Job::Clone {
                url: "/tmp/x".to_owned(),
                git_ref: Some("--upload-pack=x".to_owned()),
                update_of: None,
            },
            "is not a branch or tag",
        ),
        (
            Job::Folder {
                path: PathBuf::from("relative"),
                update_of: None,
            },
            "relative is not a folder",
        ),
    ] {
        let error = fetch(job, incoming.clone()).await.unwrap_err().to_string();
        assert!(error.contains(says), "{error}");
    }
    let empty = folders.dir.path().join("empty");
    std::fs::create_dir_all(&empty).unwrap();
    let error = fetch(
        Job::Folder {
            path: empty.clone(),
            update_of: None,
        },
        incoming.clone(),
    )
    .await
    .unwrap_err();
    assert!(matches!(error, PluginError::NoManifest { .. }), "{error}");
    let bad = folders.plugin_folder("bad", "id = \"zoe\"\n");
    let error = fetch(
        Job::Folder {
            path: bad,
            update_of: None,
        },
        incoming.clone(),
    )
    .await
    .unwrap_err()
    .to_string();
    assert!(
        error.contains("muster-plugin.toml: name is missing"),
        "{error}"
    );
    // A clone that is no plugin is let go.
    let repository = repository(folders.dir.path(), "not-a-plugin");
    let job = Job::Clone {
        url: repository.display().to_string(),
        git_ref: None,
        update_of: None,
    };
    assert!(matches!(
        fetch(job, incoming.clone()).await,
        Err(PluginError::NoManifest { .. })
    ));
    assert_eq!(std::fs::read_dir(&incoming).unwrap().count(), 0);
}

#[test]
fn a_plugin_gone_from_its_folder_is_listed_off_with_why_and_the_rest_read_on() {
    let folders = Folders::new();
    let hash = sha256(MANIFEST.as_bytes());
    let kept = folders.plugin_folder("kept", MANIFEST);
    std::fs::create_dir_all(folders.config()).unwrap();
    std::fs::write(
        folders.config().join("settings.toml"),
        format!(
            "[notch]\nshow = false\n\n[plugins.gone]\nfolder = \"/no/such/folder\"\napproved = \"{hash}\"\n\n\
             [plugins.zoe]\nfolder = \"{}\"\napproved = \"{hash}\"\n\n[plugins.never]\nfolder = \"{}\"\n\n\
             [plugins.own]\nenabled = false\n",
            kept.display(),
            kept.display()
        ),
    )
    .unwrap();
    let registry = folders.registry();
    let infos = registry.infos();
    let problem = |id: &str| {
        infos
            .iter()
            .find(|info| info.id == id)
            .map(|info| info.problem.clone())
            .unwrap()
    };
    assert_eq!(
        problem("gone"),
        Some(PluginProblem::Missing {
            path: PathBuf::from("/no/such/folder")
        })
    );
    assert_eq!(problem("zoe"), None);
    assert_eq!(problem("never"), Some(PluginProblem::NotApproved));
    assert_eq!(problem("own"), Some(PluginProblem::Unknown));
    assert_eq!(registry.list(at()).problem, None);
}

#[test]
fn settings_that_do_not_read_leave_the_plugins_folder_alone() {
    let folders = Folders::new();
    let installed = folders.data().join("plugins/zoe").join("a1".repeat(20));
    std::fs::create_dir_all(&installed).unwrap();
    std::fs::create_dir_all(folders.config()).unwrap();
    std::fs::write(
        folders.config().join("settings.toml"),
        "[notch]\nshwo = true\n",
    )
    .unwrap();
    let reading = start(&folders.data().join("plugins"), &folders.config());
    assert!(installed.is_dir());
    let mut registry = Registry::new(&folders.data(), folders.config());
    registry.take(reading);
    let list = registry.list(at());
    assert_eq!(list.plugins, []);
    assert!(list.problem.unwrap().contains("shwo"));
    // Read, they sweep what no entry names.
    std::fs::write(folders.config().join("settings.toml"), "").unwrap();
    start(&folders.data().join("plugins"), &folders.config());
    assert!(!folders.data().join("plugins/zoe").exists());
}

#[test]
fn a_manifest_that_is_a_link_or_too_large_is_refused() {
    let folders = Folders::new();
    let folder = folders.plugin_folder("zoe", MANIFEST);
    let target = folders.dir.path().join("elsewhere.toml");
    std::fs::write(&target, MANIFEST).unwrap();
    std::fs::remove_file(folder.join("muster-plugin.toml")).unwrap();
    std::os::unix::fs::symlink(&target, folder.join("muster-plugin.toml")).unwrap();
    assert!(matches!(
        read_manifest(&folder),
        Err(PluginError::Link { .. })
    ));
    std::fs::remove_file(folder.join("muster-plugin.toml")).unwrap();
    let large = format!("{MANIFEST}#{}\n", "x".repeat(70_000));
    std::fs::write(folder.join("muster-plugin.toml"), large).unwrap();
    assert!(matches!(
        read_manifest(&folder),
        Err(PluginError::TooLarge { .. })
    ));
    assert!(matches!(
        read_manifest(Path::new("/no/such")),
        Err(PluginError::NotAFolder { .. })
    ));
}

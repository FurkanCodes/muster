//! The files some harnesses read Muster's hooks from: an `OpenCode` plugin,
//! a Gemini CLI defaults file and a Copilot CLI plugin. They live in a
//! folder of Muster's own, in its private runtime directory, and the
//! harness is pointed at them for the session only: never the person's
//! config or their repository (`PRD-13`). They hold no secret: each session's
//! token reaches its program through its environment.

use std::path::{Path, PathBuf};

use super::{Kind, copilot, gemini, opencode};

/// The `OpenCode` plugin in `dir`.
pub(crate) fn opencode_plugin(dir: &Path) -> PathBuf {
    dir.join("opencode").join("muster.js")
}

/// The Gemini CLI defaults file in `dir`.
pub(crate) fn gemini_defaults(dir: &Path) -> PathBuf {
    dir.join("gemini").join("defaults.json")
}

/// The Copilot CLI plugin folder in `dir`.
pub(crate) fn copilot_plugin(dir: &Path) -> PathBuf {
    dir.join("copilot")
}

/// The files `kind` needs, with what each holds.
fn files(kind: &Kind, dir: &Path) -> Vec<(PathBuf, String)> {
    match kind {
        Kind::OpenCode => vec![(opencode_plugin(dir), opencode::PLUGIN.to_owned())],
        Kind::Gemini => vec![(gemini_defaults(dir), gemini::defaults())],
        Kind::Copilot => {
            let plugin = copilot_plugin(dir);
            vec![
                (plugin.join("plugin.json"), copilot::manifest()),
                (plugin.join("hooks").join("hooks.json"), copilot::hooks()),
            ]
        }
        Kind::Shell | Kind::Claude | Kind::Codex | Kind::Checks | Kind::Review | Kind::Added(_) => {
            Vec::new()
        }
    }
}

/// Writes the files `kind` needs into `dir`, those not already there as
/// they should be. Each is written beside its place and renamed into it,
/// so a program starting meanwhile never reads half of one.
pub(crate) fn prepare(kind: &Kind, dir: &Path) -> std::io::Result<()> {
    for (path, content) in files(kind, dir) {
        if std::fs::read_to_string(&path).is_ok_and(|current| current == content) {
            continue;
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let partial = path.with_extension("partial");
        std::fs::write(&partial, content)?;
        std::fs::rename(&partial, &path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_harness_gets_its_files_and_only_those_once() {
        let dir = tempfile::tempdir().unwrap();
        prepare(&Kind::Claude, dir.path()).unwrap();
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);

        prepare(&Kind::OpenCode, dir.path()).unwrap();
        prepare(&Kind::Gemini, dir.path()).unwrap();
        prepare(&Kind::Copilot, dir.path()).unwrap();
        let read = |path: PathBuf| std::fs::read_to_string(path).unwrap();
        assert_eq!(read(opencode_plugin(dir.path())), opencode::PLUGIN);
        assert_eq!(read(gemini_defaults(dir.path())), gemini::defaults());
        assert_eq!(
            read(copilot_plugin(dir.path()).join("hooks/hooks.json")),
            copilot::hooks()
        );

        // A changed file is put back; one as it should be is left alone.
        std::fs::write(opencode_plugin(dir.path()), "changed").unwrap();
        let defaults = gemini_defaults(dir.path());
        let before = std::fs::metadata(&defaults).unwrap().modified().unwrap();
        prepare(&Kind::OpenCode, dir.path()).unwrap();
        prepare(&Kind::Gemini, dir.path()).unwrap();
        assert_eq!(read(opencode_plugin(dir.path())), opencode::PLUGIN);
        assert_eq!(
            std::fs::metadata(&defaults).unwrap().modified().unwrap(),
            before
        );
        assert!(
            !opencode_plugin(dir.path())
                .with_extension("partial")
                .exists()
        );
    }
}

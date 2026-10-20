//! Muster's app on a Mac (`PRODUCT.md` §6): macOS shows notifications only
//! for an app, and asks the person only for one opened through its
//! launcher from where apps live, not for a program run from a terminal or
//! a temporary folder (checked 2026-10-20, macOS 26). So `muster notch
//! --detach` runs Muster's helper as `Muster.app` in Muster's data
//! directory (`~/Library/Application Support/muster`): a copy of this
//! `muster` in an app of Muster's own, signed for this machine only,
//! written again whenever this `muster` changes, and opened with `open`.
//! It carries a copy of the Mac's own sounds, as a notification's sound is
//! looked for in its app and the person's `~/Library/Sounds` only.
//! Muster's own files, beside its others, nothing of the person's
//! (`PRD-13`).

use std::path::{Path, PathBuf};

/// What macOS knows Muster's app by.
pub(crate) const BUNDLE_ID: &str = "dev.muster.Muster";

/// The app's `Info.plist`: Muster's name, this version, and no Dock icon
/// or menu bar (`LSUIElement`).
pub(crate) fn info_plist(version: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleIdentifier</key>
	<string>{BUNDLE_ID}</string>
	<key>CFBundleName</key>
	<string>Muster</string>
	<key>CFBundleDisplayName</key>
	<string>Muster</string>
	<key>CFBundleExecutable</key>
	<string>muster</string>
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleVersion</key>
	<string>{version}</string>
	<key>CFBundleShortVersionString</key>
	<string>{version}</string>
	<key>LSUIElement</key>
	<true/>
	<key>LSMinimumSystemVersion</key>
	<string>11.0</string>
</dict>
</plist>
"#
    )
}

/// What says which `muster` the app was made from: its path, size and
/// time of change, so a rebuilt or upgraded `muster` makes it again.
pub(crate) fn stamp(exe: &Path) -> std::io::Result<String> {
    let found = std::fs::metadata(exe)?;
    let changed = found
        .modified()?
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    Ok(format!("{}\n{}\n{changed}\n", exe.display(), found.len()))
}

/// Where Muster's app is, in Muster's data directory `data_dir`.
pub(crate) fn place(data_dir: &Path) -> PathBuf {
    data_dir.join("Muster.app")
}

/// Muster's app in `data_dir`, made from `exe` if it is not yet, or was
/// made from another `muster`.
pub(crate) fn ensure(data_dir: &Path, exe: &Path) -> std::io::Result<PathBuf> {
    let app = place(data_dir);
    let stamp_file = app.join("Contents/Resources/made-from");
    let wanted = stamp(exe)?;
    if std::fs::read_to_string(&stamp_file).is_ok_and(|made| made == wanted) {
        return Ok(app);
    }
    let parent = app
        .parent()
        .ok_or_else(|| std::io::Error::other("Muster's app has no folder"))?;
    std::fs::create_dir_all(parent)?;
    // Made beside it, then put in its place, so it is never half made.
    let fresh = parent.join("Muster.app.new");
    if fresh.exists() {
        std::fs::remove_dir_all(&fresh)?;
    }
    std::fs::create_dir_all(fresh.join("Contents/MacOS"))?;
    std::fs::create_dir_all(fresh.join("Contents/Resources"))?;
    std::fs::write(
        fresh.join("Contents/Info.plist"),
        info_plist(env!("CARGO_PKG_VERSION")),
    )?;
    std::fs::copy(exe, fresh.join("Contents/MacOS/muster"))?;
    carry_sounds(Path::new(MAC_SOUNDS_DIR), &fresh.join("Contents/Resources"));
    std::fs::write(fresh.join("Contents/Resources/made-from"), &wanted)?;
    // Signed for this machine alone: macOS runs no app of another's
    // making unsigned, and asks nothing of the person for this one.
    let signed = std::process::Command::new("/usr/bin/codesign")
        .args(["--force", "--sign", "-"])
        .arg(&fresh)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()?;
    if !signed.success() {
        return Err(std::io::Error::other(
            "codesign could not sign Muster's app",
        ));
    }
    if app.exists() {
        std::fs::remove_dir_all(&app)?;
    }
    std::fs::rename(&fresh, &app)?;
    Ok(app)
}

/// Where the Mac keeps its own sounds.
const MAC_SOUNDS_DIR: &str = "/System/Library/Sounds";

/// Copies the Mac's sounds Muster offers (`muster_core::telling`) from
/// `from` into `to`; one missing, or that cannot be copied, is left out,
/// and a notification with it sounds the Mac's own instead.
fn carry_sounds(from: &Path, to: &Path) {
    for name in muster_core::telling::MAC_SOUNDS {
        let file = format!("{name}.aiff");
        let _ = std::fs::copy(from.join(&file), to.join(&file));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_app_is_muster_s_without_a_dock_icon() {
        let plist = info_plist("0.4.0");
        assert!(plist.contains("<string>dev.muster.Muster</string>"));
        assert!(plist.contains("<key>LSUIElement</key>\n\t<true/>"));
        assert!(plist.contains("<string>0.4.0</string>"));
        assert!(plist.contains("<key>CFBundleExecutable</key>\n\t<string>muster</string>"));
    }

    #[test]
    fn the_app_carries_the_mac_s_sounds_it_finds() {
        let dir = tempfile::tempdir().unwrap();
        let (from, to) = (dir.path().join("system"), dir.path().join("app"));
        std::fs::create_dir_all(&from).unwrap();
        std::fs::create_dir_all(&to).unwrap();
        std::fs::write(from.join("Glass.aiff"), "glass").unwrap();
        std::fs::write(from.join("Other.aiff"), "not offered").unwrap();
        carry_sounds(&from, &to);
        assert_eq!(
            std::fs::read_to_string(to.join("Glass.aiff")).unwrap(),
            "glass"
        );
        assert!(!to.join("Other.aiff").exists());
        assert!(!to.join("Tink.aiff").exists(), "missing: left out");
    }

    #[test]
    fn the_app_is_made_again_from_another_muster() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("muster");
        std::fs::write(&exe, "one").unwrap();
        let first = stamp(&exe).unwrap();
        assert_eq!(stamp(&exe).unwrap(), first, "the same muster");
        std::fs::write(&exe, "a longer one").unwrap();
        assert_ne!(stamp(&exe).unwrap(), first);
        assert_eq!(
            place(Path::new("/Users/me/Library/Application Support/muster")),
            Path::new("/Users/me/Library/Application Support/muster/Muster.app")
        );
    }
}

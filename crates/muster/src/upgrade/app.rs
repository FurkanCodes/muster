//! `muster upgrade` on a Mac, for the Muster app (`PRODUCT.md` §6a): where
//! one is installed, as `install.sh` puts it, the release's app replaces it,
//! checked as the program is (`ARC-21`) and swapped with renames in its own
//! folder, so an app open now runs on from the files it started with.

use std::path::{Path, PathBuf};

/// The app's archive in a release: one app for both kinds of Mac.
const ARCHIVE: &str = "muster-app-universal-apple-darwin.tar.gz";

/// What became of the app.
#[derive(Debug)]
pub(super) enum App {
    /// None is installed, or this is not a Mac.
    NotInstalled,
    /// The release has no app: one from before the app.
    NotInRelease,
    /// The installed app is the release's.
    Current(String),
    New {
        from: Option<String>,
        to: String,
        at: PathBuf,
    },
}

/// The Muster app installed: in `MUSTER_APP_DIR`, `/Applications` or
/// `~/Applications`. A stand-in release (`MUSTER_RELEASES`, for tests) is
/// only ever put in `MUSTER_APP_DIR`, never in the person's own folders.
fn installed_app() -> Option<PathBuf> {
    let named = std::env::var_os("MUSTER_APP_DIR").map(PathBuf::from);
    let own: Vec<PathBuf> = if std::env::var_os("MUSTER_RELEASES").is_some() {
        Vec::new()
    } else {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        std::iter::once(PathBuf::from("/Applications"))
            .chain(home.map(|home| home.join("Applications")))
            .collect()
    };
    named
        .into_iter()
        .chain(own)
        .map(|folder| folder.join("Muster.app"))
        .find(|app| app.is_dir())
}

/// The version app `app` says it is, from its `Info.plist`.
fn version_of(app: &Path) -> Option<String> {
    let said = std::process::Command::new("/usr/libexec/PlistBuddy")
        .args(["-c", "Print CFBundleShortVersionString"])
        .arg(app.join("Contents/Info.plist"))
        .output()
        .ok()?;
    let version = String::from_utf8_lossy(&said.stdout).trim().to_owned();
    (said.status.success() && !version.is_empty()).then_some(version)
}

/// Replaces the installed Muster app with the release's at `base`, if one
/// is installed and the release has one.
pub(super) async fn upgrade(http: &reqwest::Client, base: &str) -> Result<App, String> {
    if std::env::consts::OS != "macos" {
        return Ok(App::NotInstalled);
    }
    let Some(app) = installed_app() else {
        return Ok(App::NotInstalled);
    };
    let url = format!("{base}/{ARCHIVE}");
    let Some(bytes) = super::fetch_if_there(http, &url).await? else {
        return Ok(App::NotInRelease);
    };
    println!("Downloading {url}");
    let published = super::fetch(http, &format!("{url}.sha256")).await?;
    if !super::matches(&bytes, &published) {
        return Err(
            "the Muster app does not match its published checksum, so it was not replaced"
                .to_owned(),
        );
    }
    let folder = app
        .parent()
        .ok_or_else(|| format!("{} has no folder", app.display()))?;
    // Unpacked beside the app, so the swap is a rename.
    let unpacked = tempfile::Builder::new()
        .prefix(".muster-app-upgrade-")
        .tempdir_in(folder)
        .map_err(|error| format!("cannot write in {}: {error}", folder.display()))?;
    let saved = unpacked.path().join(ARCHIVE);
    std::fs::write(&saved, &bytes).map_err(|error| error.to_string())?;
    super::say_attested(&saved).await?;
    super::untar(&saved, unpacked.path())?;
    let new = unpacked.path().join("Muster.app");
    let to = version_of(&new).ok_or("the download is not a Muster app")?;
    let from = version_of(&app);
    if from.as_deref() == Some(to.as_str()) {
        return Ok(App::Current(to));
    }
    // The old one goes into the folder that is removed after.
    let old = unpacked.path().join("Muster.app.old");
    std::fs::rename(&app, &old)
        .map_err(|error| format!("cannot move {} aside: {error}", app.display()))?;
    if let Err(error) = std::fs::rename(&new, &app) {
        let _ = std::fs::rename(&old, &app);
        return Err(format!("cannot replace {}: {error}", app.display()));
    }
    Ok(App::New { from, to, at: app })
}

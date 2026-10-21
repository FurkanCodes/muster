//! `muster upgrade`: installs the latest release over the muster running
//! now, as `install.sh` does, and moves the daemon onto it with every
//! session running (`muster reload`).
//!
//! The archive for this machine is checked against its published SHA-256
//! and, where the GitHub CLI is signed in, against the attestation GitHub
//! made as the release workflow built it, before anything is replaced; a
//! mismatch replaces nothing (`ARC-21`). The new program
//! is swapped in with a rename in its own folder, so the file is never half
//! written, and running programs keep the one they started with. On a
//! Mac the Muster app, where one is installed, is updated the same way
//! (`app`).

mod app;

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use sha2::{Digest, Sha256};

/// Where releases are, as `Cargo.toml` names the repository.
const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");

/// The release's name for this machine, as `install.sh` has it.
fn target() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Some("aarch64-apple-darwin"),
        ("macos", "x86_64") => Some("x86_64-apple-darwin"),
        ("linux", "x86_64") => Some("x86_64-unknown-linux-gnu"),
        ("linux", "aarch64") => Some("aarch64-unknown-linux-gnu"),
        _ => None,
    }
}

/// The program running now, which the new one replaces.
fn installed() -> std::io::Result<PathBuf> {
    let program = std::env::current_exe()?;
    let text = program.to_string_lossy();
    let program = match text.strip_suffix(" (deleted)") {
        Some(path) => PathBuf::from(path),
        None => program,
    };
    program.canonicalize()
}

pub(crate) async fn run(socket: &Path, runtime_dir: &Path) -> ExitCode {
    let (http, base) = match client() {
        Ok(found) => found,
        Err(error) => return crate::fail(error),
    };
    match install(&http, &base).await {
        Ok(Installed::Current(version)) => {
            println!("Muster {version} is the latest.");
        }
        Ok(Installed::New { version, at }) => {
            println!("Installed muster {version} at {}.", at.display());
        }
        Err(error) => return crate::fail(error),
    }
    match app::upgrade(&http, &base).await {
        Ok(app::App::NotInstalled) => {}
        Ok(app::App::NotInRelease) => println!("This release has no Muster app."),
        Ok(app::App::Current(version)) => println!("The Muster app {version} is the latest."),
        Ok(app::App::New { from, to, at }) => match from {
            Some(from) => println!(
                "Installed the Muster app {to} at {} (it was {from}).",
                at.display()
            ),
            None => println!("Installed the Muster app {to} at {}.", at.display()),
        },
        Err(error) => return crate::fail(error),
    }
    crate::reload::reload_daemon(socket, runtime_dir).await
}

/// The client releases are downloaded with, and where they are:
/// `MUSTER_RELEASES` points elsewhere, for tests.
fn client() -> Result<(reqwest::Client, String), String> {
    let base = std::env::var("MUSTER_RELEASES")
        .unwrap_or_else(|_| format!("{REPOSITORY}/releases/latest/download"));
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .user_agent(concat!("muster/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|error| error.to_string())?;
    Ok((http, base))
}

/// Whether `bytes` are what the checksum file `published` says.
fn matches(bytes: &[u8], published: &[u8]) -> bool {
    let expected = String::from_utf8_lossy(published)
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    expected.len() == 64 && muster_core::hex::encode(&Sha256::digest(bytes)) == expected
}

/// Checks the archive at `saved` against its build attestation, saying how
/// it went; a stand-in release, as tests use, has none.
async fn say_attested(saved: &Path) -> Result<(), String> {
    if std::env::var_os("MUSTER_RELEASES").is_some() {
        return Ok(());
    }
    if attested(saved).await? {
        println!("Checked: built by {REPOSITORY}'s release workflow.");
    } else {
        println!(
            "Not checked against its build attestation: the GitHub CLI (gh) is not installed, too \
             old, or not signed in."
        );
    }
    Ok(())
}

/// Unpacks the archive at `archive` into `into`.
fn untar(archive: &Path, into: &Path) -> Result<(), String> {
    let untarred = std::process::Command::new("tar")
        .arg("-xzf")
        .arg(archive)
        .arg("-C")
        .arg(into)
        .status()
        .map_err(|error| format!("cannot run tar: {error}"))?;
    if untarred.success() {
        Ok(())
    } else {
        Err("the archive could not be unpacked".to_owned())
    }
}

#[derive(Debug)]
enum Installed {
    /// The latest is what runs now.
    Current(String),
    New {
        version: String,
        at: PathBuf,
    },
}

async fn install(http: &reqwest::Client, base: &str) -> Result<Installed, String> {
    let target = target().ok_or_else(|| {
        format!(
            "there is no prebuilt muster for {} on {}; build it from source: cargo install \
             --locked --git {REPOSITORY} muster",
            std::env::consts::ARCH,
            std::env::consts::OS
        )
    })?;
    let archive = format!("muster-{target}.tar.gz");
    println!("Downloading {base}/{archive}");
    let bytes = fetch(http, &format!("{base}/{archive}")).await?;
    let published = fetch(http, &format!("{base}/{archive}.sha256")).await?;
    if !matches(&bytes, &published) {
        return Err(
            "the download does not match its published checksum; nothing was replaced".to_owned(),
        );
    }

    let program = installed().map_err(|error| format!("cannot find muster itself: {error}"))?;
    let folder = program
        .parent()
        .ok_or_else(|| format!("{} has no folder", program.display()))?;
    // Unpacked next to the program, so the swap is a rename.
    let unpacked = tempfile::Builder::new()
        .prefix(".muster-upgrade-")
        .tempdir_in(folder)
        .map_err(|error| format!("cannot write in {}: {error}", folder.display()))?;
    let saved = unpacked.path().join(&archive);
    std::fs::write(&saved, &bytes).map_err(|error| error.to_string())?;
    say_attested(&saved).await?;
    untar(&saved, unpacked.path())?;
    let new = unpacked
        .path()
        .join(format!("muster-{target}"))
        .join("muster");
    let version = version_of(&new)?;
    if version == env!("CARGO_PKG_VERSION") {
        return Ok(Installed::Current(version));
    }
    std::fs::set_permissions(&new, std::fs::Permissions::from_mode(0o755))
        .map_err(|error| error.to_string())?;
    std::fs::rename(&new, &program)
        .map_err(|error| format!("cannot replace {}: {error}", program.display()))?;
    Ok(Installed::New {
        version,
        at: program,
    })
}

/// Checks the archive at `path` against the attestation GitHub made as the
/// release workflow built it, where the GitHub CLI can; whether it could.
/// A mismatch is an error: the download is not what the workflow built.
async fn attested(path: &Path) -> Result<bool, String> {
    let gh = |args: &[&std::ffi::OsStr]| {
        let mut command = tokio::process::Command::new("gh");
        command
            .args(args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true);
        async move {
            tokio::time::timeout(std::time::Duration::from_mins(1), command.status())
                .await
                .ok()
                .and_then(Result::ok)
        }
    };
    if std::env::var_os("MUSTER_SKIP_ATTESTATION").is_some() {
        return Ok(false);
    }
    // A gh too old to have attestations, or not signed in, cannot check.
    for able in [
        &["attestation".as_ref(), "verify".as_ref(), "--help".as_ref()][..],
        &["auth".as_ref(), "status".as_ref()][..],
    ] {
        if !gh(able).await.is_some_and(|status| status.success()) {
            return Ok(false);
        }
    }
    let repository = REPOSITORY
        .strip_prefix("https://github.com/")
        .unwrap_or(REPOSITORY);
    let verified = gh(&[
        "attestation".as_ref(),
        "verify".as_ref(),
        path.as_os_str(),
        "--repo".as_ref(),
        repository.as_ref(),
    ])
    .await;
    match verified {
        Some(status) if status.success() => Ok(true),
        Some(_) => Err(
            "the download could not be checked against the attestation GitHub made as it was \
             built, so nothing was replaced (MUSTER_SKIP_ATTESTATION=1 upgrades with its checksum \
             checked only)"
                .to_owned(),
        ),
        None => Err(
            "gh did not finish checking the download's attestation, so nothing was replaced \
             (MUSTER_SKIP_ATTESTATION=1 upgrades with its checksum checked only)"
                .to_owned(),
        ),
    }
}

/// What `url` holds, or `None` where it is not found (404).
async fn fetch_if_there(http: &reqwest::Client, url: &str) -> Result<Option<Vec<u8>>, String> {
    let response = http
        .get(url)
        .send()
        .await
        .map_err(|error| format!("cannot download {url}: {error}"))?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !response.status().is_success() {
        return Err(format!("{url} answered {}", response.status()));
    }
    response
        .bytes()
        .await
        .map(|bytes| Some(bytes.to_vec()))
        .map_err(|error| format!("cannot download {url}: {error}"))
}

async fn fetch(http: &reqwest::Client, url: &str) -> Result<Vec<u8>, String> {
    let response = http
        .get(url)
        .send()
        .await
        .map_err(|error| format!("cannot download {url}: {error}"))?;
    if !response.status().is_success() {
        return Err(format!("{url} answered {}", response.status()));
    }
    response
        .bytes()
        .await
        .map(|bytes| bytes.to_vec())
        .map_err(|error| format!("cannot download {url}: {error}"))
}

/// The version a muster program says it is.
fn version_of(program: &Path) -> Result<String, String> {
    let said = std::process::Command::new(program)
        .arg("--version")
        .output()
        .map_err(|error| format!("the new muster does not run: {error}"))?;
    let text = String::from_utf8_lossy(&said.stdout);
    text.trim()
        .strip_prefix("muster ")
        .map(str::to_owned)
        .filter(|version| !version.is_empty() && said.status.success())
        .ok_or_else(|| "the download is not a muster program".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_machine_has_a_release_name() {
        let name = target().expect("macOS or Linux, on ARM or x86-64");
        assert!(name.ends_with("-apple-darwin") || name.ends_with("-unknown-linux-gnu"));
        assert!(REPOSITORY.starts_with("https://github.com/"));
    }
}

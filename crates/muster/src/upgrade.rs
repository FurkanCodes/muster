//! `muster upgrade`: installs the latest release over the muster running
//! now, as `install.sh` does, and moves the daemon onto it with every
//! session running (`muster reload`).
//!
//! The archive for this machine is checked against its published SHA-256
//! before anything is replaced; a mismatch replaces nothing. The new program
//! is swapped in with a rename in its own folder, so the file is never half
//! written, and running programs keep the one they started with.

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
    match install().await {
        Ok(Installed::Current(version)) => {
            println!("Muster {version} is the latest.");
        }
        Ok(Installed::New { version, at }) => {
            println!("Installed muster {version} at {}.", at.display());
        }
        Err(error) => return crate::fail(error),
    }
    crate::reload::reload_daemon(socket, runtime_dir).await
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

async fn install() -> Result<Installed, String> {
    let target = target().ok_or_else(|| {
        format!(
            "there is no prebuilt muster for {} on {}; build it from source: cargo install \
             --locked --git {REPOSITORY} muster",
            std::env::consts::ARCH,
            std::env::consts::OS
        )
    })?;
    // `MUSTER_RELEASES` points elsewhere, for tests.
    let base = std::env::var("MUSTER_RELEASES")
        .unwrap_or_else(|_| format!("{REPOSITORY}/releases/latest/download"));
    let archive = format!("muster-{target}.tar.gz");
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .user_agent(concat!("muster/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|error| error.to_string())?;
    println!("Downloading {base}/{archive}");
    let bytes = fetch(&http, &format!("{base}/{archive}")).await?;
    let published = fetch(&http, &format!("{base}/{archive}.sha256")).await?;
    let expected = String::from_utf8_lossy(&published)
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let actual: String = Sha256::digest(&bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    if expected.len() != 64 || actual != expected {
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
    let untarred = std::process::Command::new("tar")
        .arg("-xzf")
        .arg(&saved)
        .arg("-C")
        .arg(unpacked.path())
        .status()
        .map_err(|error| format!("cannot run tar: {error}"))?;
    if !untarred.success() {
        return Err("the archive could not be unpacked".to_owned());
    }
    let new = unpacked
        .path()
        .join(format!("muster-{target}"))
        .join("muster");
    let version = version_of(&new)?;
    if version == env!("CARGO_PKG_VERSION") {
        return Ok(Installed::Current(version));
    }
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&new, std::fs::Permissions::from_mode(0o755))
        .map_err(|error| error.to_string())?;
    std::fs::rename(&new, &program)
        .map_err(|error| format!("cannot replace {}: {error}", program.display()))?;
    Ok(Installed::New {
        version,
        at: program,
    })
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

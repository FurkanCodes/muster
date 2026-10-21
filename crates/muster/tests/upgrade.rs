//! End-to-end tests for `muster upgrade`, against a stand-in release on a
//! local port: a copy of muster replaces itself with the release's program
//! once its checksum matches, and is left alone when it does not.

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

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

use support::Sandbox;

/// The release's name for this machine.
fn target() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("macos", _) => "x86_64-apple-darwin",
        (_, "aarch64") => "aarch64-unknown-linux-gnu",
        _ => "x86_64-unknown-linux-gnu",
    }
}

/// Serves `files` by path on a local port; its address.
fn serve(files: HashMap<String, Vec<u8>>) -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            let _ = reader.read_line(&mut line);
            let path = line
                .split_whitespace()
                .nth(1)
                .unwrap_or_default()
                .to_owned();
            loop {
                let mut header = String::new();
                if reader.read_line(&mut header).is_err() || header.trim().is_empty() {
                    break;
                }
            }
            let (status, body) = match files.get(&path) {
                Some(body) => ("200 OK", body.clone()),
                None => ("404 Not Found", Vec::new()),
            };
            let head = format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(&body);
        }
    });
    address
}

/// A release whose program says it is muster 9.9.9, as served, with a
/// checksum file saying `checksum` (the right one if `None`).
fn release(dir: &Path, checksum: Option<&str>) -> HashMap<String, Vec<u8>> {
    let name = format!("muster-{}", target());
    let folder = dir.join(&name);
    std::fs::create_dir_all(&folder).unwrap();
    let program = folder.join("muster");
    std::fs::write(
        &program,
        "#!/bin/sh\n[ \"$1\" = --version ] && echo 'muster 9.9.9'\nexit 0\n",
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let archive = dir.join(format!("{name}.tar.gz"));
    let packed = Command::new("tar")
        .arg("-czf")
        .arg(&archive)
        .arg("-C")
        .arg(dir)
        .arg(&name)
        .status()
        .unwrap();
    assert!(packed.success());
    let bytes = std::fs::read(&archive).unwrap();
    let right = muster_core::hex::encode(&Sha256::digest(&bytes));
    let sum = format!("{}  {name}.tar.gz\n", checksum.unwrap_or(&right));
    HashMap::from([
        (format!("/{name}.tar.gz"), bytes),
        (format!("/{name}.tar.gz.sha256"), sum.into_bytes()),
    ])
}

/// A copy of muster in the sandbox, which an upgrade may replace.
fn copy_of_muster(sandbox: &Sandbox) -> PathBuf {
    let bin = sandbox.root().join("installed");
    std::fs::create_dir_all(&bin).unwrap();
    let copy = bin.join("muster");
    std::fs::copy(support::BINARY, &copy).unwrap();
    copy
}

/// Runs `program upgrade` in the sandbox's environment, releases at `base`.
fn upgrade(sandbox: &Sandbox, program: &Path, base: &str) -> (bool, String) {
    let template = sandbox.command();
    let mut command = Command::new(program);
    for (key, value) in template.get_envs() {
        if let Some(value) = value {
            command.env(key, value);
        }
    }
    let output = command
        .arg("upgrade")
        .env("MUSTER_RELEASES", base)
        .output()
        .unwrap();
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    (output.status.success(), said)
}

fn version(program: &Path) -> String {
    let output = Command::new(program).arg("--version").output().unwrap();
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

#[test]
fn an_upgrade_replaces_muster_with_the_release_once_its_checksum_matches() {
    let sandbox = Sandbox::new();
    let copy = copy_of_muster(&sandbox);
    let base = serve(release(&sandbox.root().join("release"), None));
    let (ok, said) = upgrade(&sandbox, &copy, &base);
    assert!(ok, "{said}");
    assert!(said.contains("Installed muster 9.9.9"), "{said}");
    assert!(said.contains("The daemon is not running"), "{said}");
    assert_eq!(version(&copy), "muster 9.9.9");
    // Nothing left beside it.
    let left: Vec<_> = std::fs::read_dir(copy.parent().unwrap())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(left, ["muster"]);
}

#[test]
fn a_download_that_does_not_match_its_checksum_replaces_nothing() {
    let sandbox = Sandbox::new();
    let copy = copy_of_muster(&sandbox);
    let wrong = "0".repeat(64);
    let base = serve(release(&sandbox.root().join("release"), Some(&wrong)));
    let (ok, said) = upgrade(&sandbox, &copy, &base);
    assert!(!ok, "{said}");
    assert!(
        said.contains("does not match its published checksum"),
        "{said}"
    );
    assert_eq!(
        version(&copy),
        format!("muster {}", env!("CARGO_PKG_VERSION"))
    );
}

/// A stand-in Muster app saying it is `version`, at `app`.
fn muster_app(app: &Path, version: &str) {
    let contents = app.join("Contents");
    std::fs::create_dir_all(&contents).unwrap();
    std::fs::write(
        contents.join("Info.plist"),
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD \
             PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist \
             version=\"1.0\"><dict><key>CFBundleShortVersionString</key><string>{version}</string></dict></plist>\n"
        ),
    )
    .unwrap();
}

/// What app `app` says it is.
fn app_version(app: &Path) -> String {
    let said = Command::new("/usr/libexec/PlistBuddy")
        .args(["-c", "Print CFBundleShortVersionString"])
        .arg(app.join("Contents/Info.plist"))
        .output()
        .unwrap();
    String::from_utf8_lossy(&said.stdout).trim().to_owned()
}

/// The release's Muster app, saying it is 9.9.9, with a checksum file
/// saying `checksum` (the right one if `None`).
fn app_release(dir: &Path, checksum: Option<&str>) -> HashMap<String, Vec<u8>> {
    let name = "muster-app-universal-apple-darwin.tar.gz";
    let packing = dir.join("app");
    muster_app(&packing.join("Muster.app"), "9.9.9");
    let archive = dir.join(name);
    let packed = Command::new("tar")
        .arg("-czf")
        .arg(&archive)
        .arg("-C")
        .arg(&packing)
        .arg("Muster.app")
        .status()
        .unwrap();
    assert!(packed.success());
    let bytes = std::fs::read(&archive).unwrap();
    let right = muster_core::hex::encode(&Sha256::digest(&bytes));
    let sum = format!("{}  {name}\n", checksum.unwrap_or(&right));
    HashMap::from([
        (format!("/{name}"), bytes),
        (format!("/{name}.sha256"), sum.into_bytes()),
    ])
}

/// Runs `program upgrade` with the Muster app looked for in `apps`.
fn upgrade_with_app(sandbox: &Sandbox, program: &Path, base: &str, apps: &Path) -> (bool, String) {
    let template = sandbox.command();
    let mut command = Command::new(program);
    for (key, value) in template.get_envs() {
        if let Some(value) = value {
            command.env(key, value);
        }
    }
    let output = command
        .arg("upgrade")
        .env("MUSTER_RELEASES", base)
        .env("MUSTER_APP_DIR", apps)
        .output()
        .unwrap();
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    (output.status.success(), said)
}

#[cfg(target_os = "macos")]
#[test]
fn prd_09_an_upgrade_updates_the_muster_app_installed_with_the_release_s() {
    let sandbox = Sandbox::new();
    let copy = copy_of_muster(&sandbox);
    let apps = sandbox.root().join("Applications");
    muster_app(&apps.join("Muster.app"), "0.0.1");
    let mut files = release(&sandbox.root().join("release"), None);
    files.extend(app_release(&sandbox.root().join("release"), None));
    let base = serve(files);
    let (ok, said) = upgrade_with_app(&sandbox, &copy, &base, &apps);
    assert!(ok, "{said}");
    assert!(said.contains("Installed muster 9.9.9"), "{said}");
    assert!(
        said.contains("Installed the Muster app 9.9.9") && said.contains("(it was 0.0.1)"),
        "{said}"
    );
    assert_eq!(app_version(&apps.join("Muster.app")), "9.9.9");
    // Nothing left beside it.
    let left: Vec<_> = std::fs::read_dir(&apps)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(left, ["Muster.app"]);

    // Again, from a muster of this build (the first upgrade put the
    // release's stand-in in its place): the app is the release's already.
    let copy = copy_of_muster(&sandbox);
    let (ok, said) = upgrade_with_app(&sandbox, &copy, &base, &apps);
    assert!(ok, "{said}");
    assert!(
        said.contains("The Muster app 9.9.9 is the latest"),
        "{said}"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn arc_21_a_muster_app_that_does_not_match_its_checksum_replaces_nothing() {
    let sandbox = Sandbox::new();
    let copy = copy_of_muster(&sandbox);
    let apps = sandbox.root().join("Applications");
    muster_app(&apps.join("Muster.app"), "0.0.1");
    let wrong = "0".repeat(64);
    let mut files = release(&sandbox.root().join("release"), None);
    files.extend(app_release(&sandbox.root().join("release"), Some(&wrong)));
    let (ok, said) = upgrade_with_app(&sandbox, &copy, &serve(files), &apps);
    assert!(!ok, "{said}");
    assert!(
        said.contains("the Muster app does not match its published checksum"),
        "{said}"
    );
    assert_eq!(app_version(&apps.join("Muster.app")), "0.0.1");
}

#[cfg(target_os = "macos")]
#[test]
fn a_release_without_the_app_leaves_the_muster_app_installed_alone() {
    let sandbox = Sandbox::new();
    let copy = copy_of_muster(&sandbox);
    let apps = sandbox.root().join("Applications");
    muster_app(&apps.join("Muster.app"), "0.0.1");
    let base = serve(release(&sandbox.root().join("release"), None));
    let (ok, said) = upgrade_with_app(&sandbox, &copy, &base, &apps);
    assert!(ok, "{said}");
    assert!(said.contains("This release has no Muster app"), "{said}");
    assert_eq!(app_version(&apps.join("Muster.app")), "0.0.1");
}

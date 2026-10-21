//! `ARC-01`: `muster-model` does no IO, as `muster-core` and `muster-state`
//! do none, so nothing it depends on may be a crate that does: a runtime, a
//! socket, a database, a terminal, a process. Checked against Cargo's own
//! view of the graph, so a dependency added anywhere below it is caught too.

use std::process::Command;

/// Crates that do IO; any of them below the model breaks `ARC-01`.
const IO_CRATES: &[&str] = &[
    "tokio",
    "mio",
    "socket2",
    "reqwest",
    "hyper",
    "axum",
    "rusqlite",
    "libsqlite3-sys",
    "nix",
    "rustix",
    "portable-pty",
    "sysinfo",
    "crossterm",
    "ratatui",
    "notify",
];

#[test]
fn arc_01_the_model_depends_on_nothing_that_does_io() {
    let output = Command::new(env!("CARGO"))
        .args(["tree", "--locked", "-p", "muster-model", "-e", "normal"])
        .args(["--prefix", "none", "--format", "{p}"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("cargo tree runs");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let tree = String::from_utf8_lossy(&output.stdout);
    let found: Vec<&str> = tree
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .filter(|name| IO_CRATES.contains(name))
        .collect();
    assert!(
        found.is_empty(),
        "muster-model depends on {found:?} (ARC-01)"
    );
    assert!(tree.contains("muster-model"), "{tree}");
}

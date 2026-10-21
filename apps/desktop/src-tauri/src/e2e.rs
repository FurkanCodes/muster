//! What a build for the end-to-end tests (`--features e2e`) records instead
//! of reaching outside the app: given `MUSTER_E2E_RECORD`, a folder, a web
//! address it is asked to open, a sound it is asked to play and a file it
//! is asked to show are each written there as a line (`opened`, `heard`,
//! `revealed`), and nothing is opened or played. A build without the
//! feature has no such way: [`recorded`] is always `false` there.

/// Records `what` under `kind` where the tests asked for it; whether it did.
#[cfg(feature = "e2e")]
pub(crate) fn recorded(kind: &str, what: &str) -> bool {
    use std::io::Write as _;
    let Some(folder) = std::env::var_os("MUSTER_E2E_RECORD").filter(|folder| !folder.is_empty())
    else {
        return false;
    };
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(std::path::Path::new(&folder).join(kind));
    if let Ok(mut file) = file {
        let _ = writeln!(file, "{what}");
    }
    true
}

/// A build without the `e2e` feature records nothing.
#[cfg(not(feature = "e2e"))]
pub(crate) fn recorded(_kind: &str, _what: &str) -> bool {
    false
}

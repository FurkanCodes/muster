//! Turning to the person at the command line: a question on their terminal,
//! a page in their browser.

/// Whether the person, asked `question` on their terminal, says yes;
/// never, without a terminal to ask on.
pub(crate) fn agrees(question: &str) -> std::io::Result<bool> {
    use std::io::{IsTerminal as _, Write as _};
    if !std::io::stdin().is_terminal() {
        return Ok(false);
    }
    eprint!("{question} [y/N] ");
    std::io::stderr().flush()?;
    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer)?;
    Ok(matches!(answer.trim(), "y" | "Y" | "yes"))
}

/// Opens `address` in the person's browser: whether the system's opener
/// said it did.
pub(crate) fn open_in_browser(address: &str) -> std::io::Result<bool> {
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    std::process::Command::new(opener)
        .arg(address)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
}

//! `muster auth`: giving Muster the tokens for services it reaches by their
//! APIs, kept in the macOS Keychain (`PRD-06`). A token is read without
//! being shown, or from standard input when it is piped in, and handed to
//! the Keychain without ever being on a command line.

use std::io::{BufRead, IsTerminal, Write};
use std::process::ExitCode;

/// Keeps a Bitbucket API token for `host`.
pub(crate) async fn bitbucket(host: &str) -> ExitCode {
    let host = host.trim().to_ascii_lowercase();
    if !keychain_here("BITBUCKET_TOKEN") {
        return ExitCode::FAILURE;
    }
    println!(
        "Create an API token for {host} with pull request scopes (read and write): on \
         bitbucket.org at id.atlassian.com/manage-profile/security/api-tokens, choosing \
         Bitbucket; on a Data Center, an HTTP access token. Muster opens pull requests with it."
    );
    keep(&muster_daemon::bitbucket_account(&host), "Token: ").await
}

/// Whether the Keychain is here to keep a token; if not, says what to set.
fn keychain_here(variable: &str) -> bool {
    if cfg!(target_os = "macos") {
        return true;
    }
    println!(
        "Muster keeps tokens in the macOS Keychain. Elsewhere, set {variable} in the \
         environment the daemon starts in."
    );
    false
}

/// Reads a token and keeps it for `account`.
async fn keep(account: &str, prompt: &str) -> ExitCode {
    let token = match read_secret(prompt) {
        Ok(token) if !token.trim().is_empty() => token.trim().to_owned(),
        Ok(_) => return crate::fail("no token was given"),
        Err(error) => return crate::fail(format!("could not read the token: {error}")),
    };
    match muster_daemon::keep_secret(account, &token).await {
        Ok(()) => {
            println!("Stored in the Keychain.");
            ExitCode::SUCCESS
        }
        Err(error) => crate::fail(format!("could not keep the token: {error}")),
    }
}

/// A line from the terminal, not echoed; or standard input, if piped.
fn read_secret(prompt: &str) -> std::io::Result<String> {
    let stdin = std::io::stdin();
    if !stdin.is_terminal() {
        let mut line = String::new();
        stdin.lock().read_line(&mut line)?;
        return Ok(line);
    }
    print!("{prompt}");
    std::io::stdout().flush()?;
    let silenced = silence(&stdin);
    let mut line = String::new();
    let read = stdin.lock().read_line(&mut line);
    if let Some(original) = silenced {
        let _ = nix::sys::termios::tcsetattr(&stdin, nix::sys::termios::SetArg::TCSANOW, &original);
    }
    println!();
    read.map(|_| line)
}

/// Turns the terminal's echo off, returning how it was.
fn silence(stdin: &std::io::Stdin) -> Option<nix::sys::termios::Termios> {
    use nix::sys::termios::{LocalFlags, SetArg, tcgetattr, tcsetattr};
    let original = tcgetattr(stdin).ok()?;
    let mut quiet = original.clone();
    quiet.local_flags.remove(LocalFlags::ECHO);
    tcsetattr(stdin, SetArg::TCSANOW, &quiet).ok()?;
    Some(original)
}

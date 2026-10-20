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

/// Signs in to Jira Cloud through Atlassian in the browser, or keeps a
/// token for `site` (Cloud's API token with its email, a Data Center's
/// personal access token).
pub(crate) async fn jira(token: bool, site: Option<&str>) -> ExitCode {
    if !token {
        return browser(
            muster_daemon::SignIn::Jira,
            "Atlassian",
            "JIRA_API_TOKEN and JIRA_EMAIL",
        )
        .await;
    }
    let Some(site) = site else {
        return crate::fail(
            "say which site: --site acme.atlassian.net, or a Data Center's address",
        );
    };
    let Some((host, cloud)) = muster_daemon::jira_site(site) else {
        return crate::fail(format!("{site:?} is not a Jira site's address"));
    };
    if !keychain_here("JIRA_API_TOKEN") {
        return ExitCode::FAILURE;
    }
    let email = if cloud {
        println!(
            "Create an API token at id.atlassian.com/manage-profile/security/api-tokens. Muster \
             reads, and closes when asked, the tickets of {host} with it."
        );
        match read_line("Your Atlassian email: ") {
            Ok(email) if email.contains('@') => Some(email.trim().to_owned()),
            Ok(_) => return crate::fail("that is not an email address"),
            Err(error) => return crate::fail(format!("could not read the email: {error}")),
        }
    } else {
        println!(
            "Create a personal access token in {host}'s profile settings (Personal Access Tokens)."
        );
        None
    };
    let token = match read_secret("Token: ") {
        Ok(token) if !token.trim().is_empty() => token.trim().to_owned(),
        Ok(_) => return crate::fail("no token was given"),
        Err(error) => return crate::fail(format!("could not read the token: {error}")),
    };
    match muster_daemon::keep_jira_token(&host, email, token).await {
        Ok(()) => {
            println!("Stored in the Keychain.");
            ExitCode::SUCCESS
        }
        Err(error) => crate::fail(format!("could not keep the token: {error}")),
    }
}

/// Signs in to Linear in the browser, or keeps a personal API key.
pub(crate) async fn linear(token: bool) -> ExitCode {
    if !token {
        return browser(muster_daemon::SignIn::Linear, "Linear", "LINEAR_API_KEY").await;
    }
    if !keychain_here("LINEAR_API_KEY") {
        return ExitCode::FAILURE;
    }
    println!(
        "Create a personal API key in Linear: Settings, Security & access, Personal API keys."
    );
    keep(muster_daemon::LINEAR_KEY_ACCOUNT, "API key: ").await
}

/// Keeps the secret `name` of the plugin `id`, which Muster gives that
/// plugin alone, as the variable `name`.
pub(crate) async fn plugin(id: &str, name: &str) -> ExitCode {
    if !muster_core::plugins::is_id(id) {
        return crate::fail(format!(
            "{} is not a plugin's id: 1 to 32 of a-z, 0-9 and -",
            crate::commands::shown(id)
        ));
    }
    let named = name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && name.len() <= 64;
    if !named {
        return crate::fail(format!(
            "{} is not a secret's name, as a plugin's manifest names one: letters, digits \
             and _, not starting with a digit",
            crate::commands::shown(name)
        ));
    }
    if !keychain_here(name) {
        return ExitCode::FAILURE;
    }
    println!(
        "Muster gives it to the plugin {id} as {name}, if its manifest names that secret, and \
         to nothing else."
    );
    keep(&muster_daemon::plugin_account(id, name), "Secret: ").await
}

/// Signs in to `service` in the person's browser.
async fn browser(service: muster_daemon::SignIn, name: &str, variables: &str) -> ExitCode {
    if !keychain_here(variables) {
        return ExitCode::FAILURE;
    }
    let outcome = muster_daemon::sign_in(service, |address| {
        println!("Sign in to {name} in your browser. If it does not open, go to:\n\n  {address}\n");
        let opener = if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        let _ = std::process::Command::new(opener)
            .arg(address)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    })
    .await;
    match outcome {
        Ok(()) => {
            println!("Signed in to {name}; the sign-in is kept in the Keychain.");
            ExitCode::SUCCESS
        }
        Err(error) => crate::fail(error),
    }
}

/// A line from the terminal, shown as it is typed.
fn read_line(prompt: &str) -> std::io::Result<String> {
    print!("{prompt}");
    std::io::stdout().flush()?;
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    Ok(line)
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

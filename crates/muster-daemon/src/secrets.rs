//! The tokens the person gives Muster for services it reaches by their
//! APIs (`PRD-06`, `ARCHITECTURE.md` §11): a Bitbucket token, Jira's and
//! Linear's sign-ins. They live in the macOS Keychain under the service
//! `muster`, one item per account, never in config files, the database or
//! a log; an environment variable the person sets wins over the Keychain.
//!
//! Writing one hands it to `security` on its standard input, never its
//! arguments, which anyone on the machine can read while it runs.
//! `MUSTER_KEYCHAIN=off` leaves the Keychain alone, for scripts and tests
//! that must not reach the person's own.

use std::fmt;
use std::time::Duration;

use tokio::io::AsyncWriteExt;

use crate::service::BoxFuture;

/// The Keychain items' service.
pub(crate) const SERVICE: &str = "muster";

/// How long `security` may take.
const TIME_LIMIT: Duration = Duration::from_secs(10);

/// A token, never shown.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Token(String);

impl Token {
    pub(crate) fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub(crate) fn value(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Token(<redacted>)")
    }
}

/// Where tokens are kept: the person's Keychain in use, a map in tests.
pub(crate) trait Secrets: Send + Sync + fmt::Debug {
    /// The token kept for `account`, if any.
    fn get(&self, account: &str) -> BoxFuture<'_, Option<Token>>;
    /// Keeps `token` for `account`, replacing any.
    fn set<'a>(&'a self, account: &'a str, token: &'a Token) -> BoxFuture<'a, std::io::Result<()>>;
}

/// The token for `account`: the environment's `variable`, if the person
/// set it, else the one kept.
pub(crate) async fn token(secrets: &dyn Secrets, variable: &str, account: &str) -> Option<Token> {
    if let Ok(value) = std::env::var(variable)
        && !value.trim().is_empty()
    {
        return Some(Token::new(value.trim()));
    }
    secrets.get(account).await
}

/// The macOS Keychain, through `security`.
#[derive(Debug, Default)]
pub(crate) struct Keychain;

impl Keychain {
    fn enabled() -> bool {
        cfg!(target_os = "macos") && std::env::var("MUSTER_KEYCHAIN").as_deref() != Ok("off")
    }
}

impl Secrets for Keychain {
    fn get(&self, account: &str) -> BoxFuture<'_, Option<Token>> {
        let account = account.to_owned();
        Box::pin(async move {
            if !Self::enabled() {
                return None;
            }
            let output = tokio::time::timeout(
                TIME_LIMIT,
                tokio::process::Command::new("security")
                    .args(["find-generic-password", "-s", SERVICE, "-a", &account, "-w"])
                    .stdin(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .output(),
            )
            .await
            .ok()?
            .ok()?;
            let value = String::from_utf8(output.stdout).ok()?.trim().to_owned();
            (output.status.success() && !value.is_empty()).then(|| Token::new(value))
        })
    }

    fn set<'a>(&'a self, account: &'a str, token: &'a Token) -> BoxFuture<'a, std::io::Result<()>> {
        Box::pin(async move {
            if !Self::enabled() {
                return Err(std::io::Error::other("the Keychain is off"));
            }
            let mut child = tokio::process::Command::new("security")
                .arg("-i")
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .kill_on_drop(true)
                .spawn()?;
            let mut stdin = child
                .stdin
                .take()
                .ok_or_else(|| std::io::Error::other("no stdin for security"))?;
            stdin
                .write_all(add_command(account, token.value()).as_bytes())
                .await?;
            drop(stdin);
            let status = tokio::time::timeout(TIME_LIMIT, child.wait())
                .await
                .map_err(|_| std::io::Error::other("security did not finish"))??;
            if status.success() {
                Ok(())
            } else {
                Err(std::io::Error::other("the Keychain did not keep the token"))
            }
        })
    }
}

/// The line `security -i` reads to keep `value` for `account`, each word
/// quoted as its own shell-like parser wants.
fn add_command(account: &str, value: &str) -> String {
    format!(
        "add-generic-password -U -s {} -a {} -w {}\n",
        quoted(SERVICE),
        quoted(account),
        quoted(value)
    )
}

/// `text` in double quotes, with its own quotes and backslashes escaped.
fn quoted(text: &str) -> String {
    let mut quoted = String::from("\"");
    for c in text.chars().filter(|c| !c.is_control()) {
        if c == '"' || c == '\\' {
            quoted.push('\\');
        }
        quoted.push(c);
    }
    quoted.push('"');
    quoted
}

/// Tokens in memory: for tests.
#[cfg(test)]
#[derive(Debug, Default)]
pub(crate) struct Kept(pub(crate) std::sync::Mutex<std::collections::HashMap<String, Token>>);

#[cfg(test)]
impl Secrets for Kept {
    fn get(&self, account: &str) -> BoxFuture<'_, Option<Token>> {
        let found = self
            .0
            .lock()
            .ok()
            .and_then(|kept| kept.get(account).cloned());
        Box::pin(async move { found })
    }

    fn set<'a>(&'a self, account: &'a str, token: &'a Token) -> BoxFuture<'a, std::io::Result<()>> {
        if let Ok(mut kept) = self.0.lock() {
            kept.insert(account.to_owned(), token.clone());
        }
        Box::pin(async { Ok(()) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_never_shows_its_value() {
        let token = Token::new("ATATT3xFfGF0-very-secret");
        assert!(!format!("{token:?} {:?}", Some(token.clone())).contains("very-secret"));
    }

    #[test]
    fn the_keychain_reads_its_words_quoted() {
        assert_eq!(
            add_command("bitbucket:bitbucket.org", "a\"b\\c d"),
            "add-generic-password -U -s \"muster\" -a \"bitbucket:bitbucket.org\" -w \"a\\\"b\\\\c d\"\n"
        );
        assert_eq!(quoted("x\ny"), "\"xy\"");
    }

    #[tokio::test]
    async fn the_environment_wins_over_what_is_kept() {
        let kept = Kept::default();
        kept.set("acct", &Token::new("kept")).await.unwrap();
        let variable = "MUSTER_TEST_TOKEN_THE_ENVIRONMENT_WINS";
        assert_eq!(
            token(&kept, variable, "acct").await,
            Some(Token::new("kept"))
        );
        assert_eq!(token(&kept, variable, "other").await, None);
    }
}

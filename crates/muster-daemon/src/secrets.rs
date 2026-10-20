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

/// A token or any other secret: never shown, not even by `{:?}`, so no log
/// line can carry it (`ARC-20`). It has no `Display`; `value` is the one
/// way to its text. Kept and carried as the plain string it is.
#[derive(Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub(crate) struct Token(String);

impl Token {
    pub(crate) fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub(crate) fn value(&self) -> &str {
        &self.0
    }

    pub(crate) fn into_value(self) -> String {
        self.0
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

/// The Keychain account of the plugin `id`'s secret `name`
/// (`muster auth plugin <id> <name>`).
pub(crate) fn plugin_account(id: &str, name: &str) -> String {
    format!("plugin:{id}:{name}")
}

/// The plugin `id`'s secret `name`, as its manifest names it: the one kept
/// for it, else a variable of that name the person set for the daemon.
pub(crate) async fn plugin_secret(secrets: &dyn Secrets, id: &str, name: &str) -> Option<Token> {
    if let Some(kept) = secrets.get(&plugin_account(id, name)).await {
        return Some(kept);
    }
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(|value| Token::new(value.trim()))
}

/// The longest piece of a value one Keychain item holds: `security -i`
/// reads a line of at most about 4 KB, quoting can double a value, and an
/// OAuth session with its tokens is longer than that.
const PIECE: usize = 900;

/// What a value too long for one item keeps in its first: how many pieces
/// follow, under `<account>#1`, `#2`…
const PIECES: &str = "muster-pieces:";

/// The most pieces a value is kept in.
const MAX_PIECES: usize = 64;

/// The macOS Keychain, through `security`, the one program that reads and
/// writes Muster's items, so none asks the person again after an upgrade.
#[derive(Debug, Default)]
pub(crate) struct Keychain {
    /// A keychain file to use instead of the person's own: for tests.
    file: Option<std::path::PathBuf>,
}

impl Keychain {
    fn enabled() -> bool {
        cfg!(target_os = "macos") && std::env::var("MUSTER_KEYCHAIN").as_deref() != Ok("off")
    }

    /// The item `account` holds, as it is.
    async fn read(&self, account: &str) -> Option<String> {
        let mut command = tokio::process::Command::new("security");
        command.args(["find-generic-password", "-s", SERVICE, "-a", account, "-w"]);
        if let Some(file) = &self.file {
            command.arg(file);
        }
        let output = tokio::time::timeout(
            TIME_LIMIT,
            command
                .stdin(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .kill_on_drop(true)
                .output(),
        )
        .await
        .ok()?
        .ok()?;
        let value = String::from_utf8(output.stdout).ok()?.trim().to_owned();
        (output.status.success() && !value.is_empty()).then_some(value)
    }

    /// Runs `lines` through `security -i`, on its standard input.
    async fn run(&self, lines: &str) -> std::io::Result<bool> {
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
        stdin.write_all(lines.as_bytes()).await?;
        drop(stdin);
        let status = tokio::time::timeout(TIME_LIMIT, child.wait())
            .await
            .map_err(|_| std::io::Error::other("security did not finish"))??;
        Ok(status.success())
    }
}

/// The items keeping `value` for `account`: one, or a count and its pieces.
fn items(account: &str, value: &str) -> Vec<(String, String)> {
    if value.len() <= PIECE && !value.starts_with(PIECES) {
        return vec![(account.to_owned(), value.to_owned())];
    }
    let mut pieces = Vec::new();
    let mut rest = value;
    while !rest.is_empty() {
        let mut cut = rest.len().min(PIECE);
        while !rest.is_char_boundary(cut) {
            cut -= 1;
        }
        pieces.push(rest[..cut].to_owned());
        rest = &rest[cut..];
    }
    let mut items: Vec<(String, String)> = pieces
        .into_iter()
        .enumerate()
        .map(|(at, piece)| (format!("{account}#{}", at + 1), piece))
        .collect();
    // The count last: a reader finds the pieces written before it.
    items.push((account.to_owned(), format!("{PIECES}{}", items.len())));
    items
}

/// How many pieces a first item says follow, if it says so.
fn piece_count(first: &str) -> Option<usize> {
    first.strip_prefix(PIECES)?.parse().ok()
}

impl Secrets for Keychain {
    fn get(&self, account: &str) -> BoxFuture<'_, Option<Token>> {
        let account = account.to_owned();
        Box::pin(async move {
            if !Self::enabled() {
                return None;
            }
            let first = self.read(&account).await?;
            let Some(count) = piece_count(&first) else {
                return Some(Token::new(first));
            };
            let mut value = String::new();
            for at in 1..=count.min(MAX_PIECES) {
                value.push_str(&self.read(&format!("{account}#{at}")).await?);
            }
            Some(Token::new(value))
        })
    }

    fn set<'a>(&'a self, account: &'a str, token: &'a Token) -> BoxFuture<'a, std::io::Result<()>> {
        Box::pin(async move {
            if !Self::enabled() {
                return Err(std::io::Error::other("the Keychain is off"));
            }
            let items = items(account, token.value());
            if items.len() > MAX_PIECES + 1 {
                return Err(std::io::Error::other("too long to keep in the Keychain"));
            }
            let file = self
                .file
                .as_deref()
                .map(|file| file.to_string_lossy().into_owned());
            let mut lines: String = items
                .iter()
                .map(|(account, value)| add_command(account, value, file.as_deref()))
                .collect();
            // Pieces a longer value left behind go.
            let kept = items.len().saturating_sub(1);
            for at in (kept + 1)..=MAX_PIECES {
                lines.push_str(&delete_command(&format!("{account}#{at}"), file.as_deref()));
            }
            // A line that fails (a piece not there to delete) fails the run,
            // so what was kept is read back instead.
            self.run(&lines).await?;
            match self.get(account).await {
                Some(kept) if kept == *token => Ok(()),
                _ => Err(std::io::Error::other("the Keychain did not keep the token")),
            }
        })
    }
}

/// The line `security -i` reads to keep `value` for `account` (in `file`,
/// if given), each word quoted as its own shell-like parser wants.
fn add_command(account: &str, value: &str, file: Option<&str>) -> String {
    let file = file
        .map(|file| format!(" {}", quoted(file)))
        .unwrap_or_default();
    format!(
        "add-generic-password -U -s {} -a {} -w {}{file}\n",
        quoted(SERVICE),
        quoted(account),
        quoted(value)
    )
}

/// The line that removes `account`'s item.
fn delete_command(account: &str, file: Option<&str>) -> String {
    let file = file
        .map(|file| format!(" {}", quoted(file)))
        .unwrap_or_default();
    format!(
        "delete-generic-password -s {} -a {}{file}\n",
        quoted(SERVICE),
        quoted(account)
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
    fn a_token_is_kept_as_its_plain_string_and_hidden_in_collections() {
        let token = Token::new("s3cret-value");
        let shown = format!("{:#?}", vec![Some(token.clone())]);
        assert!(!shown.contains("s3cret"), "{shown}");
        // Kept, and carried across a restart in place, as before.
        let json = serde_json::to_string(&token).unwrap();
        assert_eq!(json, "\"s3cret-value\"");
        assert_eq!(serde_json::from_str::<Token>(&json).unwrap(), token);
    }

    #[tokio::test]
    async fn a_plugin_s_secret_is_the_one_kept_else_the_daemon_s_variable() {
        let kept = Kept::default();
        // A variable no one else sets.
        let name = "MUSTER_TEST_NEVER_SET_SECRET";
        assert_eq!(plugin_secret(&kept, "youtrack", name).await, None);
        assert_eq!(
            plugin_account("youtrack", "YT_TOKEN"),
            "plugin:youtrack:YT_TOKEN"
        );
        kept.set("plugin:youtrack:PATH", &Token::new("kept"))
            .await
            .unwrap();
        assert_eq!(
            plugin_secret(&kept, "youtrack", "PATH").await,
            Some(Token::new("kept")),
            "the Keychain's wins"
        );
        assert_eq!(
            plugin_secret(&kept, "other", "PATH")
                .await
                .map(Token::into_value),
            std::env::var("PATH").ok(),
            "else the variable"
        );
    }

    #[test]
    fn a_token_never_shows_its_value() {
        let token = Token::new("ATATT3xFfGF0-very-secret");
        assert!(!format!("{token:?} {:?}", Some(token.clone())).contains("very-secret"));
    }

    #[test]
    fn the_keychain_reads_its_words_quoted() {
        assert_eq!(
            add_command("bitbucket:bitbucket.org", "a\"b\\c d", None),
            "add-generic-password -U -s \"muster\" -a \"bitbucket:bitbucket.org\" -w \"a\\\"b\\\\c d\"\n"
        );
        assert_eq!(quoted("x\ny"), "\"xy\"");
    }

    #[test]
    fn a_long_value_is_kept_in_pieces_and_a_short_one_whole() {
        assert_eq!(items("a", "short"), [("a".to_owned(), "short".to_owned())]);
        let long = "é".repeat(PIECE);
        let kept = items("jira:mcp", &long);
        assert_eq!(
            kept.last().unwrap(),
            &("jira:mcp".to_owned(), format!("{PIECES}{}", kept.len() - 1))
        );
        assert_eq!(kept[0].0, "jira:mcp#1");
        assert!(kept.iter().all(|(_, piece)| piece.len() <= PIECE));
        let joined: String = kept[..kept.len() - 1]
            .iter()
            .map(|(_, piece)| piece.as_str())
            .collect();
        assert_eq!(joined, long);
        assert_eq!(piece_count(&kept.last().unwrap().1), Some(kept.len() - 1));
        // A value that would read as a count is kept in pieces too.
        assert_eq!(items("a", "muster-pieces:3").len(), 2);
        // Every line stays well under what `security -i` reads.
        let worst = "\"".repeat(PIECE);
        assert!(add_command("jira:mcp#1", &worst, Some("/a/keychain-db")).len() < 2_000);
    }

    /// The real `security`, on a keychain of its own: `cargo test --
    /// --ignored the_real_keychain`. It makes and deletes a keychain file,
    /// so it does not run unasked.
    #[tokio::test]
    #[ignore = "makes a keychain file with the real security command"]
    async fn the_real_keychain_keeps_long_values_and_shrinks_them() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("muster-test.keychain-db");
        let run = |args: &[&str]| {
            assert!(
                std::process::Command::new("security")
                    .args(args)
                    .status()
                    .unwrap()
                    .success()
            );
        };
        let path = file.to_str().unwrap();
        run(&["create-keychain", "-p", "test", path]);
        run(&["unlock-keychain", "-p", "test", path]);
        let keychain = Keychain {
            file: Some(file.clone()),
        };
        let long = Token::new(format!("{{\"access_token\":\"{}\"}}", "x".repeat(5_000)));
        keychain.set("jira:mcp", &long).await.unwrap();
        assert_eq!(keychain.get("jira:mcp").await, Some(long));
        let short = Token::new("short");
        keychain.set("jira:mcp", &short).await.unwrap();
        assert_eq!(keychain.get("jira:mcp").await, Some(short));
        assert_eq!(keychain.read("jira:mcp#1").await, None, "old pieces go");
        run(&["delete-keychain", path]);
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

//! The credential runs' agents use (`PRD-06`, `ARCHITECTURE.md` §11): a
//! Claude Code token (`claude setup-token`) kept in the macOS Keychain, or
//! failing that `CLAUDE_CODE_OAUTH_TOKEN` or `ANTHROPIC_API_KEY` in the
//! daemon's environment. It goes only into the environment of an agent's
//! `exec` (never the container's, so the checks never see it), through
//! docker's own environment, never its arguments; its value is never
//! printed.

use std::fmt;

use crate::github::BoxFuture;

/// The Keychain item: service and account.
pub const KEYCHAIN_SERVICE: &str = "muster";
pub const KEYCHAIN_ACCOUNT: &str = "claude-code-oauth-token";

/// A credential: the variable it goes in, and a value that is never shown.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Secret {
    pub(crate) variable: &'static str,
    value: String,
}

impl Secret {
    pub(crate) fn new(variable: &'static str, value: String) -> Self {
        Self { variable, value }
    }

    pub(crate) fn value(&self) -> &str {
        &self.value
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Secret({}=<redacted>)", self.variable)
    }
}

/// Where the credential comes from: the person's in use, a fake in tests.
pub(crate) trait Credentials: Send + Sync + fmt::Debug {
    fn claude(&self) -> BoxFuture<'_, Option<Secret>>;
}

/// The Keychain, then the daemon's environment.
#[derive(Debug, Default)]
pub(crate) struct PersonsCredentials;

impl Credentials for PersonsCredentials {
    fn claude(&self) -> BoxFuture<'_, Option<Secret>> {
        Box::pin(async {
            if let Some(token) = keychain().await {
                return Some(Secret::new("CLAUDE_CODE_OAUTH_TOKEN", token));
            }
            for variable in ["CLAUDE_CODE_OAUTH_TOKEN", "ANTHROPIC_API_KEY"] {
                if let Ok(value) = std::env::var(variable)
                    && !value.trim().is_empty()
                {
                    return Some(Secret::new(variable, value.trim().to_owned()));
                }
            }
            None
        })
    }
}

/// The token in the macOS Keychain, if there is one. `MUSTER_KEYCHAIN=off`
/// leaves the Keychain alone, for scripts and tests that must not reach
/// the person's own credential.
async fn keychain() -> Option<String> {
    if !cfg!(target_os = "macos") || std::env::var("MUSTER_KEYCHAIN").as_deref() == Ok("off") {
        return None;
    }
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        tokio::process::Command::new("security")
            .args([
                "find-generic-password",
                "-s",
                KEYCHAIN_SERVICE,
                "-a",
                KEYCHAIN_ACCOUNT,
                "-w",
            ])
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .output(),
    )
    .await
    .ok()?
    .ok()?;
    let token = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    (output.status.success() && !token.is_empty()).then_some(token)
}

/// A credential fixed in advance, or none: for tests.
#[cfg(test)]
#[derive(Debug, Default)]
pub(crate) struct FixedCredentials(pub(crate) Option<Secret>);

#[cfg(test)]
impl Credentials for FixedCredentials {
    fn claude(&self) -> BoxFuture<'_, Option<Secret>> {
        let secret = self.0.clone();
        Box::pin(async move { secret })
    }
}

/// What to do when there is none.
pub(crate) const MISSING: &str = "no Claude Code credential for runs: run `claude setup-token`, \
     then `muster auth claude` and paste the token (or set CLAUDE_CODE_OAUTH_TOKEN for the daemon)";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_secret_never_shows_its_value() {
        let secret = Secret::new(
            "CLAUDE_CODE_OAUTH_TOKEN",
            "sk-ant-oat01-very-secret".to_owned(),
        );
        let shown = format!("{secret:?} {:?}", Some(secret.clone()));
        assert!(!shown.contains("very-secret"), "{shown}");
        assert!(shown.contains("CLAUDE_CODE_OAUTH_TOKEN"));
        assert_eq!(secret.value(), "sk-ant-oat01-very-secret");
    }
}

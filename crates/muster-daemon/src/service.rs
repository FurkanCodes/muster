//! What trackers and git hosts share: the futures their traits return, so
//! they can be used as objects, and why one could not answer.

use std::path::PathBuf;

use muster_core::config::ProjectConfig;
use muster_core::protocol::{ErrorBody, ErrorCode};

use crate::github::GitHubError;

/// A future some trait methods return, so the traits can be used as objects.
pub(crate) type BoxFuture<'a, T> =
    std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

/// Why a tracker or a git host could not answer.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ServiceError {
    /// The project does not say where its tickets or pull requests go, and
    /// its remote does not tell.
    #[error("{0}")]
    NotConfigured(String),
    /// The person is not signed in, or their sign-in has run out.
    #[error("{0}")]
    NotSignedIn(String),
    #[error("{0}")]
    NotFound(String),
    /// The service refused: a permission, a bad query.
    #[error("{0}")]
    Refused(String),
    /// The service, or the program reaching it, could not be reached.
    #[error("{0}")]
    Unavailable(String),
    #[error("{0}")]
    Unreadable(String),
}

impl ServiceError {
    /// The error as the protocol carries it.
    pub(crate) fn body(&self) -> ErrorBody {
        let code = match self {
            Self::NotConfigured(_) | Self::NotSignedIn(_) | Self::Refused(_) => {
                ErrorCode::BadRequest
            }
            Self::NotFound(_) => ErrorCode::NotFound,
            Self::Unavailable(_) | Self::Unreadable(_) => ErrorCode::Internal,
        };
        ErrorBody::new(code, self.to_string())
    }
}

impl From<GitHubError> for ServiceError {
    fn from(error: GitHubError) -> Self {
        let message = error.to_string();
        match error {
            GitHubError::Failed(text)
                if text.contains("Could not resolve") || text.contains("not found") =>
            {
                Self::NotFound(message)
            }
            GitHubError::Failed(text) if text.contains("gh auth login") => {
                Self::NotSignedIn(message)
            }
            GitHubError::Failed(_) => Self::Refused(message),
            GitHubError::Missing | GitHubError::Spawn(_) | GitHubError::TimedOut => {
                Self::Unavailable(message)
            }
            GitHubError::Unreadable(_) => Self::Unreadable(message),
        }
    }
}

/// A project, as its tracker or git host is found for it.
#[derive(Clone, Debug)]
pub(crate) struct ProjectPlace {
    pub(crate) root: PathBuf,
    /// The person's overrides of its `muster.toml`.
    pub(crate) overrides: PathBuf,
}

/// The project's settings, read off the loop.
pub(crate) async fn settings(place: &ProjectPlace) -> Result<ProjectConfig, ServiceError> {
    let (root, overrides) = (place.root.clone(), place.overrides.clone());
    tokio::task::spawn_blocking(move || crate::config::load(&root, &overrides))
        .await
        .map_err(|error| ServiceError::Unavailable(error.to_string()))?
        .map(|loaded| loaded.config)
        .map_err(|error| ServiceError::NotConfigured(error.to_string()))
}

/// A project's git remote, as far as it tells where it is hosted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Origin {
    /// It has no remote.
    None,
    /// Its remote's address names no host: a local path, or one Muster
    /// cannot read. The address is not kept: it may carry a password.
    Unnamed,
    Named(muster_core::remote::Remote),
}

impl Origin {
    pub(crate) fn remote(&self) -> Option<&muster_core::remote::Remote> {
        match self {
            Self::Named(remote) => Some(remote),
            Self::None | Self::Unnamed => None,
        }
    }

    /// Why it tells nothing, for a message that goes on to say what to
    /// write instead.
    pub(crate) fn why_untold(&self, what: &str) -> String {
        match self {
            Self::None => format!("this project has no git remote to find its {what} by"),
            Self::Unnamed => format!(
                "this project's git remote is not on a host Muster can name (a local \
                 path, or an address it cannot read), so it cannot tell where its {what} are"
            ),
            Self::Named(remote) => {
                format!("{} is not a host Muster knows by its name", remote.host)
            }
        }
    }
}

/// The project's remote, read.
pub(crate) async fn origin(root: &std::path::Path) -> Origin {
    match crate::git::remote_url(root).await {
        None => Origin::None,
        Some(url) => {
            muster_core::remote::Remote::parse(&url).map_or(Origin::Unnamed, Origin::Named)
        }
    }
}

/// The client for services reached by their APIs, with a time limit, so a
/// stuck service costs a request, not a command.
pub(crate) fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent(concat!("muster/", env!("CARGO_PKG_VERSION")))
        .build()
        .unwrap_or_default()
}

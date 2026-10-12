//! Each project's git host, for its pull requests: the one its settings
//! name (`[pull_requests] host`), or the one its git remote's host is.

use std::sync::Arc;

use muster_core::config::HostKind;
use muster_core::remote::Remote;

use super::GitHost;
use super::github::GitHub;
use super::gitlab::GitLab;
use crate::service::{BoxFuture, Origin, ProjectPlace, ServiceError};

/// Finds each project's git host: the real ones in use, a fake in tests.
pub(crate) trait Hosts: Send + Sync + std::fmt::Debug {
    fn for_project(
        &self,
        place: ProjectPlace,
    ) -> BoxFuture<'_, Result<Arc<dyn GitHost>, ServiceError>>;
}

/// The git hosts in use.
#[derive(Debug, Default)]
pub(crate) struct ServiceHosts;

impl Hosts for ServiceHosts {
    fn for_project(
        &self,
        place: ProjectPlace,
    ) -> BoxFuture<'_, Result<Arc<dyn GitHost>, ServiceError>> {
        Box::pin(async move {
            let config = crate::service::settings(&place).await?;
            let origin = crate::service::origin(&place.root).await;
            let host: Arc<dyn GitHost> = match choose(config.pull_requests.host, &origin)? {
                HostKind::GitHub => Arc::new(GitHub),
                HostKind::GitLab => Arc::new(GitLab),
                HostKind::Bitbucket => {
                    return Err(ServiceError::NotConfigured(
                        "Bitbucket is not yet supported".to_owned(),
                    ));
                }
            };
            Ok(host)
        })
    }
}

/// Which host a project's pull requests go to: the one written, or the
/// remote's.
pub(crate) fn choose(written: Option<HostKind>, origin: &Origin) -> Result<HostKind, ServiceError> {
    if let Some(host) = written.or_else(|| origin.remote().and_then(Remote::kind)) {
        return Ok(host);
    }
    Err(ServiceError::NotConfigured(format!(
        "{}; name its host with [pull_requests] host = \"github\", \"gitlab\" or \"bitbucket\" \
         in muster.toml",
        origin.why_untold("pull requests")
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_host_written_wins_and_else_the_remote_s() {
        let remote = |url: &str| Origin::Named(Remote::parse(url).unwrap());
        assert_eq!(
            choose(None, &remote("git@gitlab.com:g/r")).unwrap(),
            HostKind::GitLab
        );
        assert_eq!(
            choose(
                Some(HostKind::GitLab),
                &remote("https://code.example.com/g/r")
            )
            .unwrap(),
            HostKind::GitLab
        );
        let unknown = choose(None, &remote("https://code.example.com/g/r")).unwrap_err();
        assert!(
            unknown.to_string().contains("[pull_requests] host"),
            "{unknown}"
        );
        assert!(
            choose(None, &Origin::None)
                .unwrap_err()
                .to_string()
                .contains("no git remote")
        );
        assert!(
            choose(None, &Origin::Unnamed)
                .unwrap_err()
                .to_string()
                .contains("a local path")
        );
        assert_eq!(
            choose(Some(HostKind::GitHub), &Origin::Unnamed).unwrap(),
            HostKind::GitHub
        );
    }
}

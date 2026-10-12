//! Each project's tracker: the one its settings name (`[tickets]` in
//! `muster.toml`, or the person's overrides), or else the one its git
//! remote's host comes with, GitHub's or GitLab's.

use std::sync::Arc;

use muster_core::config::{HostKind, ProjectConfig, Tickets, TrackerKind};
use muster_core::remote::Remote;

use super::github::GitHubIssues;
use super::{BoxFuture, ProjectPlace, TicketSource, TrackerError, Trackers};

/// The trackers in use.
#[derive(Debug, Default)]
pub(crate) struct ServiceTrackers;

impl Trackers for ServiceTrackers {
    fn for_project(
        &self,
        place: ProjectPlace,
    ) -> BoxFuture<'_, Result<Arc<dyn TicketSource>, TrackerError>> {
        Box::pin(async move {
            let config = settings(&place).await?;
            let remote = crate::git::remote_url(&place.root)
                .await
                .and_then(|url| Remote::parse(&url));
            let tracker = choose(&config.tickets, config.pull_requests.host, remote.as_ref())?;
            let source: Arc<dyn TicketSource> = match tracker {
                TrackerKind::GitHub => Arc::new(GitHubIssues { root: place.root }),
                TrackerKind::GitLab | TrackerKind::Jira | TrackerKind::Linear => {
                    return Err(TrackerError::NotConfigured(format!(
                        "{} is not yet supported",
                        tracker.name()
                    )));
                }
            };
            Ok(source)
        })
    }
}

/// The project's settings, read off the loop.
pub(crate) async fn settings(place: &ProjectPlace) -> Result<ProjectConfig, TrackerError> {
    let (root, overrides) = (place.root.clone(), place.overrides.clone());
    tokio::task::spawn_blocking(move || crate::config::load(&root, &overrides))
        .await
        .map_err(|error| TrackerError::Unavailable(error.to_string()))?
        .map(|loaded| loaded.config)
        .map_err(|error| TrackerError::NotConfigured(error.to_string()))
}

/// Which tracker a project's tickets are in: the one written, or the one
/// its git host comes with, the host written or the remote's.
pub(crate) fn choose(
    tickets: &Tickets,
    host: Option<HostKind>,
    remote: Option<&Remote>,
) -> Result<TrackerKind, TrackerError> {
    if let Some(tracker) = tickets.tracker {
        return Ok(tracker);
    }
    const SAY: &str = "say where its tickets are with [tickets] tracker = \"github\", \"gitlab\", \
                       \"jira\" or \"linear\" in muster.toml";
    let Some(host) = host.or_else(|| remote.and_then(Remote::kind)) else {
        return Err(TrackerError::NotConfigured(match remote {
            None => format!("this project has no git remote to find its tickets by; {SAY}"),
            Some(remote) => format!(
                "{} is not a host Muster knows by its name; {SAY}, or name its host with \
                 [pull_requests] host",
                remote.host
            ),
        }));
    };
    host.own_tracker().ok_or_else(|| {
        TrackerError::NotConfigured(format!(
            "{} has no issue tracker of its own any more (its teams keep their tickets in \
             Jira); {SAY}",
            host.name()
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn remote(url: &str) -> Remote {
        Remote::parse(url).unwrap()
    }

    #[test]
    fn the_tracker_written_wins_and_else_the_hosts_own() {
        let jira = Tickets {
            tracker: Some(TrackerKind::Jira),
            ..Tickets::default()
        };
        let github = remote("git@github.com:o/r.git");
        assert_eq!(
            choose(&jira, None, Some(&github)).unwrap(),
            TrackerKind::Jira
        );
        let none = Tickets::default();
        assert_eq!(
            choose(&none, None, Some(&github)).unwrap(),
            TrackerKind::GitHub
        );
        let gitlab = remote("https://gitlab.example.com/g/r");
        assert_eq!(
            choose(&none, None, Some(&gitlab)).unwrap(),
            TrackerKind::GitLab
        );
        // A host written wins over the remote's name.
        let own = remote("https://code.example.com/g/r");
        assert_eq!(
            choose(&none, Some(HostKind::GitLab), Some(&own)).unwrap(),
            TrackerKind::GitLab
        );
    }

    #[test]
    fn where_nothing_tells_it_says_what_to_write() {
        let none = Tickets::default();
        let message =
            |remote: Option<&Remote>| choose(&none, None, remote).unwrap_err().to_string();
        assert!(message(None).contains("no git remote"));
        assert!(
            message(Some(&remote("https://code.example.com/g/r"))).contains("[pull_requests] host")
        );
        let bitbucket = message(Some(&remote("git@bitbucket.org:ws/r.git")));
        assert!(
            bitbucket.contains("Bitbucket has no issue tracker"),
            "{bitbucket}"
        );
        assert!(bitbucket.contains("tracker = \"github\", \"gitlab\", \"jira\" or \"linear\""));
    }
}

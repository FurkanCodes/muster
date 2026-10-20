//! Each project's tracker: the one its settings name (`[tickets]` in
//! `muster.toml`, or the person's overrides), or else the one its git
//! remote's host comes with, GitHub's or GitLab's. GitHub's and GitLab's
//! are Muster's own ticket plugins (`plugin`); Jira and Linear are reached
//! in the daemon still.

use std::collections::BTreeMap;
use std::sync::Arc;

use muster_core::config::{HostKind, Tickets, TrackerKind};
use muster_core::remote::Remote;

use muster_core::ticket_protocol::Project;

use super::jira::Jira;
use super::linear::Linear;
use super::plugin::{PluginTickets, Plugins};
use super::{BoxFuture, Hidden, ProjectPlace, ServiceError, TicketSource, Trackers};
use crate::secrets::Secrets;
use crate::service::Origin;

/// The trackers in use.
#[derive(Debug)]
pub(crate) struct ServiceTrackers {
    /// The sign-ins and tokens for trackers reached by MCP or their APIs.
    pub(crate) secrets: Arc<dyn Secrets>,
    pub(crate) http: reqwest::Client,
    /// The ticket plugins' processes.
    pub(crate) plugins: Plugins,
}

impl Trackers for ServiceTrackers {
    fn for_project(
        &self,
        place: ProjectPlace,
    ) -> BoxFuture<'_, Result<Arc<dyn TicketSource>, ServiceError>> {
        Box::pin(async move {
            let config = crate::service::settings(&place).await?;
            let origin = crate::service::origin(&place.root).await;
            let tracker = choose(&config.tickets, config.pull_requests.host, &origin)?;
            let source: Arc<dyn TicketSource> = match tracker {
                TrackerKind::GitHub | TrackerKind::GitLab => Arc::new(PluginTickets::new(
                    self.plugins.builtin(tracker),
                    plugin_project(&place, &origin)?,
                    config.tickets.mine,
                )),
                TrackerKind::Jira => {
                    Jira::for_settings(&config.tickets, self.secrets.as_ref(), self.http.clone())
                        .await?
                }
                TrackerKind::Linear => {
                    Linear::for_settings(&config.tickets, self.secrets.as_ref(), self.http.clone())
                        .await?
                }
            };
            // Nothing reads a ticket but through this: its credentials go.
            let hidden: Arc<dyn TicketSource> = Arc::new(Hidden(source));
            Ok(hidden)
        })
    }
}

/// The project as a ticket plugin is told of it. Muster's own GitHub and
/// GitLab take no settings: their CLIs find the repository from the
/// checkout.
fn plugin_project(place: &ProjectPlace, origin: &Origin) -> Result<Project, ServiceError> {
    let remote = origin
        .remote()
        .map(|remote| format!("{}/{}", remote.host, remote.path));
    Project::new(&place.root, remote, BTreeMap::new()).ok_or_else(|| {
        ServiceError::NotConfigured(
            "this project's folder has a name that is not UTF-8, which a ticket plugin \
             cannot be told"
                .to_owned(),
        )
    })
}

/// How to say where a project's tickets are.
const SAY: &str = "choose where its tickets are on the Settings screen, or say with \
                   [tickets] tracker = \"github\", \"gitlab\", \"jira\" or \"linear\" in \
                   muster.toml";

/// Which tracker a project's tickets are in: the one written, or the one
/// its git host comes with, the host written or the remote's.
pub(crate) fn choose(
    tickets: &Tickets,
    host: Option<HostKind>,
    origin: &Origin,
) -> Result<TrackerKind, ServiceError> {
    if let Some(tracker) = tickets.tracker {
        return Ok(tracker);
    }
    let Some(host) = host.or_else(|| origin.remote().and_then(Remote::kind)) else {
        let why = origin.why_untold("tickets");
        return Err(ServiceError::NotConfigured(match origin {
            Origin::Named(_) => format!("{why}; {SAY}, or name its host with [pull_requests] host"),
            Origin::None | Origin::Unnamed => format!("{why}; {SAY}"),
        }));
    };
    host.own_tracker().ok_or_else(|| {
        ServiceError::NotConfigured(format!(
            "{} has no issue tracker of its own any more (its teams keep their tickets in \
             Jira); {SAY}",
            host.name()
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn remote(url: &str) -> Origin {
        Origin::Named(Remote::parse(url).unwrap())
    }

    #[test]
    fn the_tracker_written_wins_and_else_the_hosts_own() {
        let jira = Tickets {
            tracker: Some(TrackerKind::Jira),
            ..Tickets::default()
        };
        let github = remote("git@github.com:o/r.git");
        assert_eq!(choose(&jira, None, &github).unwrap(), TrackerKind::Jira);
        let none = Tickets::default();
        assert_eq!(choose(&none, None, &github).unwrap(), TrackerKind::GitHub);
        let gitlab = remote("https://gitlab.example.com/g/r");
        assert_eq!(choose(&none, None, &gitlab).unwrap(), TrackerKind::GitLab);
        // A host written wins over the remote's name.
        let own = remote("https://code.example.com/g/r");
        assert_eq!(
            choose(&none, Some(HostKind::GitLab), &own).unwrap(),
            TrackerKind::GitLab
        );
        assert_eq!(
            choose(&none, Some(HostKind::GitHub), &Origin::Unnamed).unwrap(),
            TrackerKind::GitHub
        );
    }

    #[test]
    fn where_nothing_tells_it_says_what_to_write() {
        let none = Tickets::default();
        let message = |origin: &Origin| choose(&none, None, origin).unwrap_err().to_string();
        assert!(message(&Origin::None).contains("no git remote"));
        assert!(message(&Origin::Unnamed).contains("a local path"));
        assert!(message(&remote("https://code.example.com/g/r")).contains("[pull_requests] host"));
        let bitbucket = message(&remote("git@bitbucket.org:ws/r.git"));
        assert!(
            bitbucket.contains("Bitbucket has no issue tracker"),
            "{bitbucket}"
        );
        assert!(bitbucket.contains("tracker = \"github\", \"gitlab\", \"jira\" or \"linear\""));
    }
}

//! Each project's tracker: the one its settings name (`[tickets]` in
//! `muster.toml`, or the person's overrides), or else the one its git
//! remote's host comes with, GitHub's or GitLab's. A tracker is a ticket
//! plugin, found by its id among those the registry has: Muster's own
//! GitHub and GitLab, and anyone's, run out of process (`plugin`); Jira
//! and Linear are listed with them, and reached in the daemon still.

use std::collections::BTreeMap;
use std::sync::Arc;

use muster_core::config::{HostKind, Tickets, TrackerId};
use muster_core::plugins::{TicketSource as Declared, own};
use muster_core::remote::Remote;
use muster_core::ticket_protocol::Project;

use super::jira::Jira;
use super::linear::Linear;
use super::plugin::{Environment, PluginSpec, PluginTickets, Plugins, TicketPlugin};
use super::{BoxFuture, Hidden, ProjectPlace, ServiceError, TicketSource, Trackers};
use crate::secrets::{Secrets, Token};
use crate::service::Origin;

/// The trackers in use.
#[derive(Debug)]
pub(crate) struct ServiceTrackers {
    /// The sign-ins and tokens for trackers reached by MCP or their APIs,
    /// and plugins' secrets.
    pub(crate) secrets: Arc<dyn Secrets>,
    pub(crate) http: reqwest::Client,
    /// The ticket plugins, as the registry last said, and their processes.
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
            let plugin = self.plugins.find(&tracker)?;
            let declared = plugin.runnable()?;
            let source: Arc<dyn TicketSource> = match tracker.as_str() {
                own::JIRA => {
                    Jira::for_settings(&config.tickets, self.secrets.as_ref(), self.http.clone())
                        .await?
                }
                own::LINEAR => {
                    Linear::for_settings(&config.tickets, self.secrets.as_ref(), self.http.clone())
                        .await?
                }
                _ => {
                    // Muster's own were checked as the settings were read.
                    muster_core::config::check_fields(
                        &tracker,
                        &declared.settings,
                        &config.tickets.fields,
                    )
                    .map_err(ServiceError::NotConfigured)?;
                    let spec = self.spec(&plugin, declared).await;
                    Arc::new(PluginTickets::new(
                        self.plugins.process(spec),
                        plugin_project(&place, &origin, config.tickets.fields.clone())?,
                        config.tickets.mine,
                    ))
                }
            };
            // Nothing reads a ticket but through this: its credentials go.
            let hidden: Arc<dyn TicketSource> = Arc::new(Hidden(source));
            Ok(hidden)
        })
    }
}

impl ServiceTrackers {
    /// How to run `plugin`, whose ticket source is `declared`: anyone's
    /// given the person's variables its manifest names and its secrets,
    /// each from the Keychain, else the daemon's environment.
    async fn spec(&self, plugin: &TicketPlugin, declared: &Declared) -> PluginSpec {
        let given = |name: &String| {
            std::env::var(name)
                .ok()
                .map(|value| (name.clone(), Token::new(value)))
        };
        let variables = plugin.env.iter().filter_map(given).collect();
        let mut secrets = Vec::new();
        for name in &plugin.secrets {
            if let Some(value) =
                crate::secrets::plugin_secret(self.secrets.as_ref(), &plugin.id, name).await
            {
                secrets.push((name.clone(), value));
            }
        }
        PluginSpec::of(
            plugin,
            declared,
            self.plugins.program(),
            Environment::Only { variables, secrets },
        )
    }
}

/// The project as a ticket plugin is told of it, with what its `[tickets]`
/// says for the plugin.
fn plugin_project(
    place: &ProjectPlace,
    origin: &Origin,
    settings: BTreeMap<String, String>,
) -> Result<Project, ServiceError> {
    let remote = origin
        .remote()
        .map(|remote| format!("{}/{}", remote.host, remote.path));
    Project::new(&place.root, remote, settings).ok_or_else(|| {
        ServiceError::NotConfigured(
            "this project's folder has a name that is not UTF-8, which a ticket plugin \
             cannot be told"
                .to_owned(),
        )
    })
}

/// How to say where a project's tickets are.
const SAY: &str = "choose where its tickets are on the Settings screen, or say with \
                   [tickets] tracker = \"github\", \"gitlab\", \"jira\", \"linear\" or a ticket \
                   plugin's id in muster.toml";

/// Which tracker a project's tickets are in, by its plugin's id: the one
/// written, or the one its git host comes with, the host written or the
/// remote's.
pub(crate) fn choose(
    tickets: &Tickets,
    host: Option<HostKind>,
    origin: &Origin,
) -> Result<TrackerId, ServiceError> {
    if let Some(tracker) = &tickets.tracker {
        return Ok(tracker.clone());
    }
    let Some(host) = host.or_else(|| origin.remote().and_then(Remote::kind)) else {
        let why = origin.why_untold("tickets");
        return Err(ServiceError::NotConfigured(match origin {
            Origin::Named(_) => format!("{why}; {SAY}, or name its host with [pull_requests] host"),
            Origin::None | Origin::Unnamed => format!("{why}; {SAY}"),
        }));
    };
    host.own_tracker().map(str::to_owned).ok_or_else(|| {
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
            tracker: Some("jira".to_owned()),
            ..Tickets::default()
        };
        let github = remote("git@github.com:o/r.git");
        assert_eq!(choose(&jira, None, &github).unwrap(), "jira");
        let none = Tickets::default();
        assert_eq!(choose(&none, None, &github).unwrap(), "github");
        let gitlab = remote("https://gitlab.example.com/g/r");
        assert_eq!(choose(&none, None, &gitlab).unwrap(), "gitlab");
        // A host written wins over the remote's name.
        let own = remote("https://code.example.com/g/r");
        assert_eq!(
            choose(&none, Some(HostKind::GitLab), &own).unwrap(),
            "gitlab"
        );
        assert_eq!(
            choose(&none, Some(HostKind::GitHub), &Origin::Unnamed).unwrap(),
            "github"
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
        assert!(
            bitbucket
                .contains("tracker = \"github\", \"gitlab\", \"jira\", \"linear\" or a ticket")
        );
    }
}

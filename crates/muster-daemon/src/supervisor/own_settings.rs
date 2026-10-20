//! Muster's own settings, the person's for all of it: whether the notch
//! shows, and the coding agents they added (`[agents.<name>]`), which are
//! offered and started beside Muster's own. Read as the daemon starts,
//! after a change on the Settings screen, and when the person asks for them
//! again after editing the file by hand (`Command::ReadPreferences`).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use muster_core::config::agents::AgentDefinition;
use muster_core::protocol::{Delta, ErrorBody, ErrorCode, Reply};
use muster_core::settings::Preferences;

use super::Supervisor;
use crate::harness::{self, Harness};

/// Muster's own settings in `config_dir` and the agents the person added,
/// read off the caller's loop.
pub(super) async fn read(config_dir: PathBuf) -> (Preferences, Vec<Arc<AgentDefinition>>) {
    let file = config_dir.join(muster_core::config::preferences::FILE_NAME);
    match tokio::task::spawn_blocking(move || settled(&config_dir)).await {
        Ok(read) => read,
        Err(error) => {
            tracing::warn!(%error, "could not read Muster's own settings");
            let preferences = Preferences {
                file,
                ..Preferences::default()
            };
            (preferences, Vec::new())
        }
    }
}

/// Muster's own settings in `config_dir`, or, where they do not read,
/// Muster's defaults and no agent of the person's, with why: logged, and
/// told to clients so the person can see which key is wrong (`ARC-05`).
fn settled(config_dir: &Path) -> (Preferences, Vec<Arc<AgentDefinition>>) {
    match crate::config::own_settings(config_dir) {
        Ok(own) => (
            own.preferences,
            own.agents.into_iter().map(Arc::new).collect(),
        ),
        Err(error) => {
            tracing::warn!(%error, "Muster's own settings do not read; using its defaults");
            let preferences = Preferences {
                file: config_dir.join(muster_core::config::preferences::FILE_NAME),
                problem: Some(error.to_string()),
                ..Preferences::default()
            };
            (preferences, Vec::new())
        }
    }
}

impl Supervisor {
    /// The harness called `id`: Muster's own, or an agent the person added.
    pub(super) fn harness(&self, id: &str) -> Option<Harness> {
        harness::find_with(id, &self.agents)
    }

    /// The harness called `id`, or a refusal saying what there is.
    pub(super) fn known_harness(&self, id: &str) -> Result<Harness, ErrorBody> {
        self.harness(id).ok_or_else(|| self.no_harness(id))
    }

    /// Why there is no harness called `id`, with what would help.
    pub(super) fn no_harness(&self, id: &str) -> ErrorBody {
        let preferences = &self.state.preferences;
        let help = match &preferences.problem {
            Some(problem) => format!("; Muster's own settings do not read: {problem}"),
            None if self.agents.is_empty() => format!(
                "; add an agent of your own as [agents.{id}] in {}",
                preferences.file.display()
            ),
            None => {
                let names: Vec<&str> = self.agents.iter().map(|a| a.name.as_str()).collect();
                format!("; yours are {}", names.join(", "))
            }
        };
        ErrorBody::new(
            ErrorCode::BadRequest,
            format!("no harness called {id:?}{help}"),
        )
    }

    /// The harness of a session that ran as `id`, to keep it running
    /// (`harness::carried`): a plugin's panel stays a panel, and an agent
    /// the settings no longer define is known by its name alone.
    pub(super) fn running_harness(&self, id: &str) -> Harness {
        harness::carried(id, &self.agents)
    }

    /// Takes Muster's own settings as read, and tells every client.
    pub(super) fn take_own_settings(
        &mut self,
        (preferences, agents): (Preferences, Vec<Arc<AgentDefinition>>),
    ) {
        self.agents = agents;
        self.state.preferences = preferences.clone();
        self.broadcast(Delta::Preferences(preferences));
    }

    /// `ReadPreferences`: reads Muster's own settings again, as the person
    /// edited them. A file that does not read is no refusal: the clients
    /// are told why, and the defaults hold meanwhile.
    pub(super) fn read_preferences(&mut self) -> Reply {
        let read = settled(&self.config_dir);
        tracing::info!(
            agents = read.1.len(),
            reads = read.0.problem.is_none(),
            "read Muster's own settings again"
        );
        self.take_own_settings(read);
        // The plugins are in the same file, and their manifests are read
        // again with it.
        self.read_plugins();
        Reply::Done
    }
}

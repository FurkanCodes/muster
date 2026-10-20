//! The plugins on the Supervisor's side (`ARCHITECTURE.md` §7a): the
//! person's commands, the registry read as the daemon starts, when the
//! person has Muster's settings read again and for each listing, and
//! every client told as it changes. Fetching runs off the loop; changes
//! to Muster's settings are made on it, so two never interleave.

use std::path::PathBuf;
use std::time::Duration;

use muster_core::protocol::plugins::PluginCommand;
use muster_core::protocol::{Delta, ErrorBody, Reply};
use tokio::sync::oneshot;

use super::Supervisor;
use super::handle::Request;
use crate::plugins::{self, Fetched, Job, PluginError};

/// How long a fetch may take in all: git's own limit is two minutes.
const FETCH_LIMIT: Duration = Duration::from_mins(3);

impl Supervisor {
    /// The registry as the daemon starts, read off the loop.
    pub(super) async fn start_plugins(&mut self) {
        let (dir, config_dir) = self.plugins.folders();
        match tokio::task::spawn_blocking(move || plugins::start(&dir, &config_dir)).await {
            Ok(reading) => {
                self.plugins.take(reading);
                self.state.plugins = self.plugins.infos();
                self.publish_ticket_plugins();
            }
            Err(error) => tracing::warn!(%error, "could not read the plugins"),
        }
    }

    /// `Command::Plugin`: answered here, or, for what fetches, from a task
    /// of its own.
    pub(super) async fn plugin_command(
        &mut self,
        command: PluginCommand,
        reply: oneshot::Sender<Result<Reply, ErrorBody>>,
    ) {
        let (now, at) = (self.clock.instant(), self.clock.now());
        let outcome = match command {
            PluginCommand::OpenPanel {
                plugin,
                panel,
                beside,
                size,
            } => {
                let opened = self.open_panel(&plugin, &panel, beside, size).await;
                let _ = reply.send(opened);
                return;
            }
            PluginCommand::List => {
                self.read_plugins();
                return answer(reply, Ok(Reply::Plugins(self.plugins.list(at))));
            }
            PluginCommand::Preview { origin } => {
                use muster_core::protocol::plugins::PluginOrigin;
                let job = match origin {
                    PluginOrigin::Git { url, git_ref } => Job::Clone {
                        url,
                        git_ref,
                        update_of: None,
                    },
                    PluginOrigin::Folder { path } => Job::Folder {
                        path,
                        update_of: None,
                    },
                };
                return self.fetch_plugin(job, reply);
            }
            PluginCommand::Update { id } => match self.plugins.update_job(&id) {
                Ok(job) => return self.fetch_plugin(job, reply),
                Err(error) => Err(error),
            },
            PluginCommand::Install { preview, by } => {
                let (outcome, gone) = self.plugins.install(&preview, by, now, at);
                self.remove_later(gone);
                outcome
            }
            PluginCommand::SetEnabled { id, on, by } => self.plugins.set_enabled(&id, on, by, at),
            PluginCommand::Remove { id } => {
                let (outcome, gone) = self.plugins.remove(&id);
                self.remove_later(gone);
                outcome
            }
            PluginCommand::Log { id } => {
                let outcome = self.plugins.infos().iter().any(|info| info.id == id);
                return answer(
                    reply,
                    if outcome {
                        let log = self.ticket_plugins.log(&id);
                        Ok(Reply::PluginLog { id, log })
                    } else {
                        Err(PluginError::NoPlugin { id })
                    },
                );
            }
        };
        if outcome.is_ok() {
            self.read_plugins_and_tell();
        }
        answer(reply, outcome.map(|()| Reply::Done));
    }

    /// Fetches what `job` names off the loop, back as
    /// `Request::PluginFetched`.
    fn fetch_plugin(&self, job: Job, reply: oneshot::Sender<Result<Reply, ErrorBody>>) {
        let incoming = self.plugins.incoming();
        let requests = self.requests.clone();
        self.tasks.spawn(async move {
            // Every wait in it has a deadline, git's own and this (`ARC-13`).
            let outcome = tokio::time::timeout(FETCH_LIMIT, plugins::fetch(job, incoming))
                .await
                .unwrap_or_else(|_| {
                    Err(PluginError::Fetch(
                        "it took longer than three minutes".to_owned(),
                    ))
                });
            let fetched = Fetched { outcome, reply };
            if let Err(unsent) = requests
                .send(Request::PluginFetched(Box::new(fetched)))
                .await
                && let Request::PluginFetched(fetched) = unsent.0
                && let Ok(found) = fetched.outcome
                && let Some(clone) = found.clone
            {
                // The daemon is stopping: its next start empties
                // `.incoming` too.
                plugins::remove_later(clone).await;
            }
        });
    }

    /// A fetch came back: held as a preview, or refused.
    pub(super) fn plugin_fetched(&mut self, fetched: Fetched) {
        let outcome = match fetched.outcome {
            Ok(found) => {
                let (preview, gone) = self.plugins.hold(found, self.clock.instant());
                self.remove_later(gone);
                preview.map(|preview| Reply::PluginPreview(Box::new(preview)))
            }
            Err(error) => Err(error),
        };
        answer(fetched.reply, outcome);
    }

    /// Reads the registry again, on the person's word; every client is
    /// told if anything changed.
    pub(super) fn read_plugins(&mut self) {
        let reading = self.plugins.read();
        let changed = self.plugins.take(reading);
        self.publish_ticket_plugins();
        if changed {
            self.tell_plugins();
        }
    }

    /// Reads the registry after a change and tells every client.
    fn read_plugins_and_tell(&mut self) {
        let reading = self.plugins.read();
        self.plugins.take(reading);
        self.publish_ticket_plugins();
        self.tell_plugins();
    }

    /// Tells the ticket plugins what the registry says: a plugin turned
    /// off, moved or gone has its process stopped, off the loop
    /// (`ARC-14`).
    fn publish_ticket_plugins(&self) {
        for process in self.ticket_plugins.publish(self.plugins.ticket_plugins()) {
            self.tasks.spawn(async move { process.stop().await });
        }
    }

    fn tell_plugins(&mut self) {
        let infos = self.plugins.infos();
        self.state.plugins.clone_from(&infos);
        self.broadcast(Delta::PluginsChanged(infos));
    }

    /// Removes the folders a change let go of, off the loop.
    fn remove_later(&self, gone: Vec<PathBuf>) {
        for folder in gone {
            self.tasks.spawn(plugins::remove_later(folder));
        }
    }
}

/// Answers a plugin command, logging a refusal.
fn answer(reply: oneshot::Sender<Result<Reply, ErrorBody>>, outcome: Result<Reply, PluginError>) {
    let outcome = outcome.map_err(|error| {
        tracing::info!(command = "plugin", %error, "command refused");
        error.body()
    });
    let _ = reply.send(outcome);
}

//! The settings a person changes on the Settings screen (`ARCHITECTURE.md`
//! §12). Reading them happens off the Supervisor's loop, since telling
//! where a project's tickets come from asks git for its remote. A change
//! is made on the loop, so two changes to one file never interleave, and
//! only in the person's own overrides of the project's `muster.toml`,
//! never in the repository (`ARC-05`).

use muster_core::ProjectId;
use muster_core::protocol::{ErrorBody, ErrorCode, Reply};
use muster_core::settings::{ProjectSettings, SettingChange, TicketsFrom, Toggle};
use muster_core::text::one_line;
use tokio::sync::oneshot;

use super::Supervisor;
use crate::trackers::ProjectPlace;
use crate::trackers::choose::choose;

impl Supervisor {
    /// Reads a project's settings on a task of its own.
    pub(super) fn project_settings(
        &self,
        project: ProjectId,
        reply: oneshot::Sender<Result<Reply, ErrorBody>>,
    ) {
        let place = match self.tracker_place(project) {
            Ok(place) => place,
            Err(error) => {
                let _ = reply.send(Err(error));
                return;
            }
        };
        self.tasks.spawn(async move {
            let outcome = settings_of(place).await;
            if let Err(error) = &outcome {
                tracing::info!(command = "project settings", %error, "command refused");
            }
            let _ = reply.send(outcome);
        });
    }

    /// Makes a change in the person's settings for a project, then reads
    /// them back on a task of their own.
    pub(super) fn change_setting(
        &self,
        project: ProjectId,
        change: &SettingChange,
        reply: oneshot::Sender<Result<Reply, ErrorBody>>,
    ) {
        let place = match self.tracker_place(project) {
            Ok(place) => place,
            Err(error) => {
                let _ = reply.send(Err(error));
                return;
            }
        };
        if let Err(error) = crate::config::change(&place.root, &place.overrides, change) {
            tracing::info!(command = "change setting", %error, "command refused");
            let _ = reply.send(Err(ErrorBody::new(
                ErrorCode::BadRequest,
                error.to_string(),
            )));
            return;
        }
        tracing::info!(%project, ?change, "changed a setting");
        self.tasks.spawn(async move {
            let _ = reply.send(settings_of(place).await);
        });
    }
}

/// The project's settings that the person can change, as they stand.
async fn settings_of(place: ProjectPlace) -> Result<Reply, ErrorBody> {
    let (root, overrides) = (place.root.clone(), place.overrides.clone());
    let (config, sources) =
        tokio::task::spawn_blocking(move || crate::config::load_with_sources(&root, &overrides))
            .await
            .map_err(|error| ErrorBody::new(ErrorCode::Internal, error.to_string()))?
            .map_err(|error| ErrorBody::new(ErrorCode::BadRequest, error.to_string()))?;
    let written = &config.tickets;
    let tickets = written.tracker.map(|tracker| TicketsFrom {
        tracker,
        site: written.site.clone(),
        project: written.project.clone(),
        jql: written.jql.clone(),
        team: written.team.clone(),
    });
    let (tickets_from, tickets_untold) = if let Some(written) = config.tickets.written() {
        (written, false)
    } else {
        let origin = crate::service::origin(&place.root).await;
        let host = config.pull_requests.host;
        match (choose(&config.tickets, host, &origin), host) {
            (Ok(tracker), Some(host)) => (
                format!(
                    "{}, as its pull requests are on {}",
                    tracker.name(),
                    host.name()
                ),
                false,
            ),
            (Ok(tracker), None) => (format!("{}, as its git remote says", tracker.name()), false),
            (Err(error), _) => (error.to_string(), true),
        }
    };
    Ok(Reply::ProjectSettings(ProjectSettings {
        tickets_from: one_line(&tickets_from),
        only_mine: Toggle {
            on: config.tickets.mine,
            from: sources.only_mine,
        },
        file: place.overrides,
        tickets,
        tickets_set_by: sources.tracker,
        tickets_untold,
        muster_prefix: Some(Toggle {
            on: config.branches.muster_prefix,
            from: sources.muster_prefix,
        }),
    }))
}

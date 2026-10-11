//! The project's checks by hand (`PRODUCT.md` §5, "By hand"): run in a
//! hands-on workspace as a session of their own, so the person watches
//! them, runs them again by starting the session again, and finds a
//! failure among the news. They run on the host, in the workspace, as the
//! person, with the settings in the workspace's own `muster.toml` (the
//! branch may change its checks) and the person's overrides.

use std::path::{Path, PathBuf};

use muster_core::protocol::{ErrorBody, ErrorCode, Reply, TermSize};
use muster_core::{SessionStatus, WorkspaceId};
use muster_state::session::Start;

use super::Supervisor;
use super::sessions::Mode;
use crate::harness::{self, CheckCommands, Place};

impl Supervisor {
    /// Starts a session running the project's checks in `workspace`.
    pub(super) async fn check_workspace(
        &mut self,
        workspace: WorkspaceId,
        size: TermSize,
    ) -> Result<Reply, ErrorBody> {
        let harness = harness::find("checks").ok_or_else(|| {
            ErrorBody::new(ErrorCode::Internal, "the checks are not a known harness")
        })?;
        let place = self.place_for(workspace, harness)?;
        let Place::Host(path) = &place else {
            return Err(ErrorBody::new(
                ErrorCode::BadRequest,
                "checks by hand run in a worktree or the project's checkout",
            ));
        };
        let checks = self.checks_in(workspace, path).await?;
        let id = self
            .add_session(workspace, harness, None, Start::WithPrompt)
            .await?;
        if let Err(error) = self.launch(
            id,
            harness,
            place,
            size,
            Mode::Checks(checks),
            Start::WithPrompt,
            false,
        ) {
            self.end_session(id, SessionStatus::Exited { success: false }, true)
                .await;
            return Err(ErrorBody::new(
                ErrorCode::Internal,
                format!("could not start the checks: {error}"),
            ));
        }
        Ok(Reply::SessionStarted { id })
    }

    /// The checks the settings in the workspace at `path` name.
    pub(super) async fn checks_in(
        &self,
        workspace: WorkspaceId,
        path: &Path,
    ) -> Result<CheckCommands, ErrorBody> {
        let project = self
            .state
            .workspaces
            .iter()
            .find(|candidate| candidate.id == workspace)
            .map(|found| found.project)
            .ok_or_else(|| {
                ErrorBody::new(ErrorCode::NotFound, format!("no workspace {workspace}"))
            })?;
        let overrides: PathBuf = self
            .config_dir
            .join("projects")
            .join(format!("{project}.toml"));
        let root = path.to_owned();
        let loaded = tokio::task::spawn_blocking(move || crate::config::load(&root, &overrides))
            .await
            .map_err(|error| ErrorBody::new(ErrorCode::Internal, error.to_string()))?
            .map_err(|error| ErrorBody::new(ErrorCode::BadRequest, error.to_string()))?;
        let checks = loaded.config.checks;
        if checks.commands.is_empty() {
            return Err(ErrorBody::new(
                ErrorCode::BadRequest,
                "the project names no checks: add `[checks] commands = [\"…\"]` to muster.toml",
            ));
        }
        Ok(CheckCommands {
            commands: checks.commands,
            limit: checks.timeout,
        })
    }
}

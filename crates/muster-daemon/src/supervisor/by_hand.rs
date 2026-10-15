//! The stages as tools on a hands-on workspace (`PRODUCT.md` §5, "By
//! hand"), on the host, as the person:
//!
//! - the project's checks, run as a session of their own, so the person
//!   watches them, runs them again by starting the session again, and
//!   finds a failure among the news; with the settings in the workspace's
//!   own `muster.toml` (the branch may change its checks) and the person's
//!   overrides;
//! - a review of the workspace's change by Claude Code in plan mode, which
//!   changes nothing, as a session the person reads;
//! - a draft pull request: the branch pushed, as committed, with the
//!   person's own git, and the pull request opened with their `gh`, or
//!   found open, off the Supervisor's loop.

use std::path::{Path, PathBuf};

use muster_core::protocol::{ErrorBody, ErrorCode, Reply, TermSize};
use muster_core::{SessionStatus, WorkspaceId};
use muster_state::session::Start;
use tokio::sync::oneshot;

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

impl Supervisor {
    /// Starts Claude Code reviewing the change in `workspace`.
    pub(super) async fn review_workspace(
        &mut self,
        workspace: WorkspaceId,
        size: TermSize,
    ) -> Result<Reply, ErrorBody> {
        let harness = harness::find("review").ok_or_else(|| {
            ErrorBody::new(ErrorCode::Internal, "the review is not a known harness")
        })?;
        let place = self.place_for(workspace, harness)?;
        let Place::Host(path) = &place else {
            return Err(ErrorBody::new(
                ErrorCode::BadRequest,
                "a review by hand runs in a worktree or the project's checkout",
            ));
        };
        let branch = self.reviewable_branch(workspace)?;
        let configured = self.settings_in(workspace, path).await?.base_branch;
        let base = base_of(path, configured.as_deref()).await;
        let prompt = crate::runs::prompts::review_by_hand(&branch, &base);
        let id = self
            .add_session(workspace, harness, None, Start::WithPrompt)
            .await?;
        let mode = Mode::Fresh {
            prompt: Some(prompt),
        };
        if let Err(error) = self.launch(id, harness, place, size, mode, Start::WithPrompt, false) {
            self.end_session(id, SessionStatus::Exited { success: false }, true)
                .await;
            return Err(ErrorBody::new(
                ErrorCode::Internal,
                format!("could not start the review: {error}"),
            ));
        }
        Ok(Reply::SessionStarted { id })
    }

    /// Pushes `workspace`'s branch and opens a draft pull request from it,
    /// off the loop: the network is slow.
    pub(super) fn open_pull_request(
        &self,
        workspace: WorkspaceId,
        reply: oneshot::Sender<Result<Reply, ErrorBody>>,
    ) {
        let prepared = (|| {
            let found = self
                .state
                .workspaces
                .iter()
                .find(|candidate| candidate.id == workspace)
                .ok_or_else(|| {
                    ErrorBody::new(ErrorCode::NotFound, format!("no workspace {workspace}"))
                })?;
            let path = found.kind.host_path().map(Path::to_owned).ok_or_else(|| {
                ErrorBody::new(
                    ErrorCode::BadRequest,
                    "a run opens its own pull request; hand it back to have it opened",
                )
            })?;
            let branch = self.reviewable_branch(workspace)?;
            let overrides = self
                .config_dir
                .join("projects")
                .join(format!("{}.toml", found.project));
            Ok::<_, ErrorBody>((path, branch, overrides))
        })();
        let (path, branch, overrides) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                let _ = reply.send(Err(error));
                return;
            }
        };
        let (hosts, requests) = (self.hosts.clone(), self.requests.clone());
        let place = crate::service::ProjectPlace {
            root: path.clone(),
            overrides: overrides.clone(),
        };
        tokio::spawn(async move {
            let opened = async {
                // Which host first: a project it cannot tell is refused
                // before anything is pushed.
                let host = hosts
                    .for_project(place)
                    .await
                    .map_err(|error| error.body())?;
                let configured = load_settings(path.clone(), overrides).await?.base_branch;
                let base = base_of(&path, configured.as_deref()).await;
                let base = base.strip_prefix("origin/").unwrap_or(&base).to_owned();
                if base == branch {
                    return Err(ErrorBody::new(
                        ErrorCode::BadRequest,
                        format!("{branch} is the branch a pull request would merge into"),
                    ));
                }
                let refspec = format!("refs/heads/{branch}:refs/heads/{branch}");
                crate::git::checked(&path, &["push", "--quiet", "origin", &refspec])
                    .await
                    .map_err(|error| {
                        ErrorBody::new(
                            ErrorCode::BadRequest,
                            format!("could not push {branch}: {error}"),
                        )
                    })?;
                let title = crate::git::checked(&path, &["log", "-1", "--format=%s"])
                    .await
                    .unwrap_or_else(|_| branch.clone());
                let body = "Opened from Muster by hand: no checks or review were asked for yet.";
                let pr = host
                    .open_draft(&path, &branch, &base, &title, body)
                    .await
                    .map_err(|error| error.body())?;
                Ok(Reply::PullRequestOpened { pr })
            };
            let outcome = opened.await;
            match &outcome {
                // Shown beside the worktree at once, not at the next look.
                Ok(_) => {
                    let _ = requests.send(super::Request::LookAtBranch(workspace)).await;
                }
                Err(error) => {
                    tracing::info!(command = "open pull request", %error, "command refused");
                }
            }
            let _ = reply.send(outcome);
        });
    }

    /// The branch checked out in `workspace`, if it can go anywhere: not
    /// detached, and not a run's container.
    fn reviewable_branch(&self, workspace: WorkspaceId) -> Result<String, ErrorBody> {
        let found = self
            .state
            .workspaces
            .iter()
            .find(|candidate| candidate.id == workspace)
            .ok_or_else(|| {
                ErrorBody::new(ErrorCode::NotFound, format!("no workspace {workspace}"))
            })?;
        if !found.kind.in_git() {
            return Err(ErrorBody::new(
                ErrorCode::BadRequest,
                "that is a plain folder, not a git repository: it has no branch to review or push",
            ));
        }
        let branch = found.kind.branch().to_owned();
        if branch.starts_with("detached at ") || branch == "?" {
            return Err(ErrorBody::new(
                ErrorCode::BadRequest,
                format!("there is no branch checked out there ({branch})"),
            ));
        }
        Ok(branch)
    }

    /// The settings in the workspace at `path`, with the person's
    /// overrides.
    async fn settings_in(
        &self,
        workspace: WorkspaceId,
        path: &Path,
    ) -> Result<muster_core::config::ProjectConfig, ErrorBody> {
        let project = self
            .state
            .workspaces
            .iter()
            .find(|candidate| candidate.id == workspace)
            .map(|found| found.project)
            .ok_or_else(|| {
                ErrorBody::new(ErrorCode::NotFound, format!("no workspace {workspace}"))
            })?;
        let overrides = self
            .config_dir
            .join("projects")
            .join(format!("{project}.toml"));
        load_settings(path.to_owned(), overrides).await
    }
}

/// The settings of the repository at `root`, with the overrides.
pub(super) async fn load_settings(
    root: PathBuf,
    overrides: PathBuf,
) -> Result<muster_core::config::ProjectConfig, ErrorBody> {
    tokio::task::spawn_blocking(move || crate::config::load(&root, &overrides))
        .await
        .map_err(|error| ErrorBody::new(ErrorCode::Internal, error.to_string()))?
        .map(|loaded| loaded.config)
        .map_err(|error| ErrorBody::new(ErrorCode::BadRequest, error.to_string()))
}

/// What a change in the repository at `path` is to merge into: the base
/// the settings name, or the remote's default branch, as the remote has it
/// (`origin/<base>`) where it can be; `main` failing all else.
pub(super) async fn base_of(path: &Path, configured: Option<&str>) -> String {
    let remote = |branch: &str| format!("refs/remotes/origin/{branch}");
    if let Some(branch) = configured {
        let has = crate::git::answer(path, &["show-ref", "--verify", "--quiet", &remote(branch)])
            .await
            .ok()
            .flatten()
            .is_some();
        return if has {
            format!("origin/{branch}")
        } else {
            branch.to_owned()
        };
    }
    crate::git::answer(
        path,
        &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"],
    )
    .await
    .ok()
    .flatten()
    .filter(|head| !head.is_empty())
    .unwrap_or_else(|| "main".to_owned())
}

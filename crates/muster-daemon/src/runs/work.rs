//! Runs' work, done for real (`ARCHITECTURE.md` §8): the mirror, the
//! container, headless agents and checks as jobs in it, commits out as a
//! bundle and pushed from the host, the pull request and its review.
//! Each piece runs on a task of its own and reports back; each can be
//! asked for again after a restart and finds what was already started;
//! each stops without a word once the run has moved past the generation it
//! was asked under, rather than start anything more.

mod agents;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use muster_core::{ProjectId, PullRequest, RunId, SessionId};
use muster_state::run::{RunEffect, RunEvent};

use super::credentials::{Credentials, MISSING, Secret};
use super::job;
use super::provision::{self, STATE_DIR};
use super::{Report, Reports, RunContext, RunWork, image, mirror};
use crate::containers::{Docker, LABEL_RUN, RuntimeError};
use crate::githost::{GitHost, PrState};
use crate::tasks::Tasks;

/// How long a quick question to a container may take.
const QUICK: Duration = Duration::from_secs(60);
/// How long docker may be unreachable before a job's work is given up.
const DOCKER_PATIENCE: Duration = Duration::from_mins(10);

/// Does runs' work with Docker, git and the git host.
#[derive(Clone, Debug)]
pub(crate) struct DockerWork {
    inner: Arc<Inner>,
    tasks: Tasks,
}

#[derive(Debug)]
struct Inner {
    docker: Docker,
    data_dir: PathBuf,
    host: Arc<dyn GitHost>,
    credentials: Arc<dyn Credentials>,
    /// One change to a project's mirror at a time.
    mirrors: Mutex<HashMap<ProjectId, Arc<tokio::sync::Mutex<()>>>>,
    claude_version: tokio::sync::OnceCell<String>,
    /// How often a job's output is looked at.
    poll: Duration,
    /// An image to use instead of building one (`MUSTER_RUN_IMAGE`).
    image: Option<String>,
}

/// Why a piece of work did not get done.
#[derive(Debug)]
enum Halt {
    /// The run moved on: the work is no longer wanted, and says nothing.
    Overtaken,
    /// It failed: the run stops for the person, with why.
    Failed {
        reason: String,
        /// The container is gone, and the work in it.
        container_lost: bool,
    },
}

impl Halt {
    fn failed(reason: impl Into<String>) -> Self {
        Self::Failed {
            reason: reason.into(),
            container_lost: false,
        }
    }

    /// The report for the run, if any.
    fn report(self, generation: u64) -> Option<Report> {
        match self {
            Self::Overtaken => None,
            Self::Failed {
                reason,
                container_lost,
            } => Some(Report::Event(RunEvent::InfraFailed {
                generation,
                reason,
                container_lost,
            })),
        }
    }
}

impl From<RuntimeError> for Halt {
    fn from(error: RuntimeError) -> Self {
        match error {
            RuntimeError::NoContainer(_) => Self::Failed {
                reason: "the run's container is gone".to_owned(),
                container_lost: true,
            },
            error => Self::failed(error.to_string()),
        }
    }
}

impl DockerWork {
    /// The work, with an `image` to use instead of building one, if given.
    pub(crate) fn new(
        data_dir: PathBuf,
        host: Arc<dyn GitHost>,
        credentials: Arc<dyn Credentials>,
        poll: Duration,
        image: Option<String>,
        tasks: Tasks,
    ) -> Self {
        Self {
            tasks,
            inner: Arc::new(Inner {
                docker: Docker::default(),
                data_dir,
                host,
                credentials,
                mirrors: Mutex::new(HashMap::new()),
                claude_version: tokio::sync::OnceCell::new(),
                poll,
                image,
            }),
        }
    }
}

impl RunWork for DockerWork {
    fn start(&self, context: RunContext, effect: RunEffect, reports: Reports) {
        let inner = self.inner.clone();
        self.tasks.spawn(async move {
            let run = context.run;
            let send = |report| {
                let _ = reports.send((run, report));
            };
            let (generation, outcome) = match effect {
                RunEffect::StartContainer { generation } => {
                    let started = inner.start_container(&context, generation).await;
                    let event = started.map(|container| {
                        send(Report::Container(container));
                        RunEvent::ContainerReady { generation }
                    });
                    (generation, event)
                }
                RunEffect::StartBuilder {
                    generation,
                    attempt,
                    feedback,
                } => {
                    let built = inner
                        .build(&context, generation, attempt, feedback.as_ref(), &reports)
                        .await;
                    let event = built.map(|outcome| RunEvent::AgentFinished {
                        generation,
                        outcome,
                    });
                    (generation, event)
                }
                RunEffect::RunChecks { generation } => {
                    let checked = inner.check(&context, generation).await;
                    let event = checked.map(|(passed, report)| RunEvent::ChecksFinished {
                        generation,
                        passed,
                        report,
                    });
                    (generation, event)
                }
                RunEffect::PublishBranch { generation } => {
                    let published = inner.publish(&context, generation).await;
                    let event = published.map(|pr| {
                        let number = pr.number;
                        send(Report::PullRequest(pr));
                        RunEvent::PrOpened { generation, number }
                    });
                    (generation, event)
                }
                RunEffect::StartReviewer { generation } => {
                    let reviewed = inner.review(&context, generation, &reports).await;
                    let event = reviewed.map(|verdict| RunEvent::ReviewSubmitted {
                        generation,
                        verdict,
                    });
                    (generation, event)
                }
                RunEffect::MarkReady => return inner.mark(&context, true).await,
                RunEffect::MarkDraft => return inner.mark(&context, false).await,
                RunEffect::StopWork { before } => {
                    if let Some(container) = &context.container
                        && let Err(error) = job::stop_before(&inner.docker, container, before).await
                    {
                        tracing::warn!(%run, %error, "could not stop a run's work");
                    }
                    return;
                }
                RunEffect::StartInteractive => {
                    // The Supervisor starts the person's session itself.
                    tracing::warn!(%run, "asked to start an agent for the person");
                    return;
                }
                RunEffect::RemoveContainer => {
                    return inner.remove_containers(&HashSet::from([run])).await;
                }
                RunEffect::Transition { .. } => return,
            };
            match outcome {
                Ok(event) => send(Report::Event(event)),
                Err(halt) => {
                    if let Some(report) = halt.report(generation) {
                        send(report);
                    }
                }
            }
        });
    }

    fn stop_session(&self, container: String, session: SessionId) {
        let inner = self.inner.clone();
        self.tasks.spawn(async move {
            let script = format!(
                "{}{}",
                crate::harness::IN_CONTAINER,
                crate::harness::STOP_IN_CONTAINER
            );
            let session = session.to_string();
            let stopped = inner
                .docker
                .exec(
                    &container,
                    &["sh", "-c", &script, "sh", &session],
                    &[],
                    QUICK,
                )
                .await;
            if let Err(error) = stopped {
                tracing::debug!(%session, %error, "could not stop a session in its container");
            }
        });
    }

    fn remove_containers_of(&self, runs: Vec<RunId>) {
        if runs.is_empty() {
            return;
        }
        let inner = self.inner.clone();
        self.tasks.spawn(async move {
            inner.remove_containers(&runs.into_iter().collect()).await;
        });
    }

    fn check_pull_request(&self, context: RunContext, reports: Reports) {
        let inner = self.inner.clone();
        self.tasks.spawn(async move {
            let Some(pr) = &context.pr else {
                return;
            };
            match inner.host.state(&context.root, pr.number).await {
                Ok(PrState::Merged) => {
                    let _ = reports.send((context.run, Report::Event(RunEvent::PrMerged)));
                }
                Ok(PrState::Closed) => {
                    let _ = reports.send((context.run, Report::Event(RunEvent::PrClosed)));
                }
                Ok(PrState::Open) => {}
                Err(error) => {
                    tracing::debug!(run = %context.run, %error, "could not look at a pull request");
                }
            }
        });
    }
}

impl Inner {
    fn mirror_lock(&self, project: ProjectId) -> Arc<tokio::sync::Mutex<()>> {
        let mut mirrors = self
            .mirrors
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        mirrors.entry(project).or_default().clone()
    }

    /// Brings the mirror up to date, with the person's branch for a run
    /// that took one over.
    async fn refresh_mirror(&self, context: &RunContext) -> Result<mirror::Mirror, Halt> {
        let lock = self.mirror_lock(context.project);
        let _held = lock.lock().await;
        let mirror = mirror::refresh(&self.data_dir, context.project, &context.root)
            .await
            .map_err(|error| {
                Halt::failed(format!(
                    "could not update Muster's copy of the repository: {error}"
                ))
            })?;
        if context.from_branch {
            mirror
                .take_local(&context.root, &context.branch)
                .await
                .map_err(|error| {
                    Halt::failed(format!("could not take {}: {error}", context.branch))
                })?;
        }
        Ok(mirror)
    }

    async fn start_container(&self, context: &RunContext, generation: u64) -> Result<String, Halt> {
        let mirror = self.refresh_mirror(context).await?;
        let image = if let Some(image) = &self.image {
            image.clone()
        } else {
            let version = self.claude_version.get_or_init(image::claude_version).await;
            image::image_for(
                &self.docker,
                &context.root,
                context.config.dockerfile.as_deref(),
                version,
            )
            .await
            .map_err(Halt::failed)?
        };
        // Building the image can take long; the run may have ended since.
        if context.overtaken(generation) {
            return Err(Halt::Overtaken);
        }
        let identity = provision::identity(&context.root).await;
        let provisioned = provision::provision(&self.docker, context, &mirror, &image, &identity)
            .await
            .map_err(Halt::failed)?;
        Ok(provisioned.container)
    }

    /// The run's container, running; it is started again if it stopped.
    async fn container(&self, context: &RunContext) -> Result<String, Halt> {
        let lost = || Halt::Failed {
            reason: "the run's container is gone".to_owned(),
            container_lost: true,
        };
        let Some(container) = context.container.clone() else {
            return Err(lost());
        };
        let found = self
            .docker
            .find(LABEL_RUN, &context.run.to_string())
            .await?;
        match found.into_iter().find(|candidate| {
            container.starts_with(&candidate.id) || candidate.id.starts_with(&container)
        }) {
            Some(running) if running.running => Ok(container),
            Some(_) => {
                self.docker.restart(&container).await?;
                Ok(container)
            }
            None => Err(lost()),
        }
    }

    async fn secret(&self) -> Result<Secret, Halt> {
        self.credentials
            .claude()
            .await
            .ok_or_else(|| Halt::failed(MISSING))
    }

    /// Takes in what the run's branch gained elsewhere (a suggestion
    /// committed on GitHub, an update from its base, the person's own
    /// commits on a branch they handed over) before more work goes on it,
    /// so what is checked is what is pushed and the push is not refused.
    /// Work left uncommitted is committed first. A merge that conflicts
    /// stops the run for the person.
    async fn sync(&self, context: &RunContext, container: &str) -> Result<(), Halt> {
        if context.pr.is_none() && !context.from_branch {
            // Nothing but the run has worked on its branch.
            return Ok(());
        }
        self.refresh_mirror(context).await?;
        let synced = self
            .docker
            .exec(
                container,
                &["sh", "-c", SYNC, "sh", &context.branch],
                &[],
                QUICK * 5,
            )
            .await?;
        match synced.code {
            Some(0) => Ok(()),
            Some(3) => Err(Halt::failed(format!(
                "{} has commits elsewhere that conflict with the run's work; take the run over \
                 to merge them",
                context.branch
            ))),
            _ => Err(Halt::failed(format!(
                "could not bring in what {} gained elsewhere: {}",
                context.branch,
                provision::last_lines(&synced.stderr, 4)
            ))),
        }
    }

    async fn publish(&self, context: &RunContext, generation: u64) -> Result<PullRequest, Halt> {
        let container = self.container(context).await?;
        let base = self.base(&container).await?;
        let branch = &context.branch;
        let bundle = self
            .docker
            .exec(
                &container,
                &["sh", "-c", BUNDLE, "sh", branch, &base],
                &[],
                QUICK * 5,
            )
            .await?;
        if !bundle.success() || bundle.stdout.is_empty() {
            return Err(Halt::failed(format!(
                "could not take the commits out of the container: {}",
                provision::last_lines(&bundle.stderr, 4)
            )));
        }
        let file = self
            .data_dir
            .join("bundles")
            .join(format!("{}-{generation}.bundle", context.run));
        if let Some(parent) = file.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|error| Halt::failed(error.to_string()))?;
        }
        tokio::fs::write(&file, &bundle.stdout)
            .await
            .map_err(|error| Halt::failed(error.to_string()))?;
        if context.overtaken(generation) {
            let _ = tokio::fs::remove_file(&file).await;
            return Err(Halt::Overtaken);
        }
        let pushed = async {
            let lock = self.mirror_lock(context.project);
            let _held = lock.lock().await;
            let mirror = mirror::Mirror {
                path: mirror::path(&self.data_dir, context.project),
            };
            mirror.take_bundle(&file, branch).await?;
            mirror.push(branch).await
        }
        .await;
        let _ = tokio::fs::remove_file(&file).await;
        pushed.map_err(|error| {
            Halt::failed(format!(
                "could not push {branch}: {error}; a retry checks the branch again with what it \
                 gained elsewhere"
            ))
        })?;
        if context.overtaken(generation) {
            return Err(Halt::Overtaken);
        }
        let title = format!("{} (#{})", context.ticket.title, context.ticket.number);
        let body = format!(
            "Closes #{number}.\n\nMade by a Muster run on {url}; the project's checks passed:\n\n{checks}\n\nA reviewer agent reviews it next; a person decides.",
            number = context.ticket.number,
            url = context.ticket.url,
            checks = context
                .config
                .checks
                .commands
                .iter()
                .map(|command| format!("- `{command}`"))
                .collect::<Vec<_>>()
                .join("\n"),
        );
        self.host
            .open_draft(&context.root, branch, &base, &title, &body)
            .await
            .map_err(|error| Halt::failed(format!("could not open the pull request: {error}")))
    }

    async fn mark(&self, context: &RunContext, ready: bool) {
        let Some(pr) = &context.pr else {
            return;
        };
        let marked = if ready {
            self.host.mark_ready(&context.root, pr.number).await
        } else {
            self.host.mark_draft(&context.root, pr.number).await
        };
        if let Err(error) = marked {
            tracing::warn!(run = %context.run, %error, ready, "could not change the pull request");
        }
    }

    /// Removes the containers of `runs`, whatever they are called.
    async fn remove_containers(&self, runs: &HashSet<RunId>) {
        let labelled = match self.docker.labelled(LABEL_RUN).await {
            Ok(labelled) => labelled,
            // Without Docker there is nothing of it to remove.
            Err(error @ (RuntimeError::NotRunning(_) | RuntimeError::Missing)) => {
                tracing::debug!(%error, "could not list runs' containers to remove");
                return;
            }
            Err(error) => {
                tracing::warn!(%error, "could not list runs' containers to remove");
                return;
            }
        };
        for (found, value) in labelled {
            if !runs.iter().any(|run| run.to_string() == value) {
                continue;
            }
            if let Err(error) = self.docker.remove(&found.id).await {
                tracing::warn!(container = %found.name, %error, "could not remove a run's container");
            }
        }
    }

    /// The branch the run started from, as provisioning recorded it.
    async fn base(&self, container: &str) -> Result<String, Halt> {
        let file = format!("{STATE_DIR}/base");
        let output = self
            .docker
            .exec(container, &["cat", &file], &[], QUICK)
            .await?;
        let base = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if !output.success() || base.is_empty() {
            return Err(Halt::failed(
                "the container does not say which branch the run started from",
            ));
        }
        Ok(base)
    }
}

/// Brings what the run's branch gained elsewhere into the container's
/// clone: `$1` is the branch. Exits 3 when the merge conflicts (and is
/// undone), 2 when the mirror cannot be reached.
const SYNC: &str = r#"branch="$1"
cd /work || exit 2
git add --all
git diff --cached --quiet || git commit --quiet --message "Work left uncommitted"
git fetch --quiet origin || exit 2
if git rev-parse --quiet --verify "refs/remotes/origin/$branch" >/dev/null \
   && ! git merge-base --is-ancestor "origin/$branch" HEAD; then
  if ! git merge --quiet --no-edit "origin/$branch" >/dev/null 2>&1; then
    git merge --abort >/dev/null 2>&1
    exit 3
  fi
fi
exit 0
"#;

/// The run's commits as a bundle on stdout: `$1` the branch, `$2` the base.
const BUNDLE: &str = r#"cd /work
git add --all
git diff --cached --quiet || git commit --quiet --message "Work the agent left uncommitted"
git bundle create - "refs/heads/$1" "^origin/$2"
"#;

#[cfg(test)]
mod tests;
